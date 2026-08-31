use std::ptr;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use super::job::{ft_job_create, ft_job_release, ft_job_set_result_cb, ft_tunnel_start_job};
use super::*;

struct SendPtr<T>(*mut T);

// SAFETY: the tests keep the pointed-to ABI handle alive until every spawned
// thread has joined.
unsafe impl<T> Send for SendPtr<T> {}

impl<T> SendPtr<T> {
    fn get(&self) -> *mut T {
        self.0
    }
}

#[derive(Default)]
struct BlockingInvocation {
    state: Mutex<InvocationState>,
    changed: Condvar,
}

#[derive(Default)]
struct InvocationState {
    entered: bool,
    released: bool,
}

impl BlockingInvocation {
    fn block(&self) {
        let mut state = self.state.lock().expect("callback state mutex poisoned");
        state.entered = true;
        self.changed.notify_all();
        while !state.released {
            state = self
                .changed
                .wait(state)
                .expect("callback state mutex poisoned");
        }
    }

    fn wait_until_entered(&self) {
        let state = self.state.lock().expect("callback state mutex poisoned");
        let (state, timeout) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(1), |state| !state.entered)
            .expect("callback state mutex poisoned");
        assert!(state.entered, "callback did not start before timeout");
        assert!(!timeout.timed_out());
    }

    fn release(&self) {
        let mut state = self.state.lock().expect("callback state mutex poisoned");
        state.released = true;
        self.changed.notify_all();
    }
}

extern "C" fn blocking_status_callback(
    user: *mut c_void,
    _status: c_int,
    _progress: c_int,
    _error: c_int,
    _message: *const c_char,
) {
    // SAFETY: the test keeps this context alive until the callback and setter
    // threads have joined.
    unsafe { &*user.cast::<BlockingInvocation>() }.block();
}

extern "C" fn blocking_result_callback(user: *mut c_void, _result: FtJobResult) {
    // SAFETY: the test keeps this context alive until the callback and setter
    // threads have joined.
    unsafe { &*user.cast::<BlockingInvocation>() }.block();
}

struct RecursiveStatusInvocation {
    tunnel: usize,
    calls: AtomicUsize,
    nested_status: AtomicI32,
}

extern "C" fn recursive_status_callback(
    user: *mut c_void,
    _status: c_int,
    _progress: c_int,
    _error: c_int,
    _message: *const c_char,
) {
    // SAFETY: the test keeps this context and tunnel alive until dispatch joins.
    let context = unsafe { &*user.cast::<RecursiveStatusInvocation>() };
    if context.calls.fetch_add(1, Ordering::SeqCst) == 0 {
        // SAFETY: the tunnel is live and its callback context remains valid.
        let status = unsafe {
            ft_tunnel_start_connect(context.tunnel as *mut FtTunnelHandle, None, ptr::null_mut())
        };
        context.nested_status.store(status, Ordering::SeqCst);
    }
}

struct RecursiveResultInvocation {
    tunnel: usize,
    job: usize,
    calls: AtomicUsize,
    nested_status: AtomicI32,
}

extern "C" fn recursive_result_callback(user: *mut c_void, _result: FtJobResult) {
    // SAFETY: the test keeps this context and both handles alive until dispatch joins.
    let context = unsafe { &*user.cast::<RecursiveResultInvocation>() };
    if context.calls.fetch_add(1, Ordering::SeqCst) == 0 {
        // SAFETY: both handles are live and the callback context remains valid.
        let status = unsafe {
            ft_tunnel_start_job(
                context.tunnel as *mut FtTunnelHandle,
                context.job as *mut FtJobHandle,
            )
        };
        context.nested_status.store(status, Ordering::SeqCst);
    }
}

#[test]
fn status_callback_replacement_waits_for_in_flight_invocation() {
    let mut tunnel = ptr::null_mut();
    let blocker = Box::into_raw(Box::<BlockingInvocation>::default());
    // SAFETY: output and callback context are valid until the calls finish.
    unsafe {
        assert_eq!(ft_tunnel_create(ptr::null(), &mut tunnel), FT_OK);
        assert_eq!(
            ft_tunnel_set_status_cb(
                tunnel,
                Some(blocking_status_callback),
                blocker.cast::<c_void>(),
            ),
            FT_OK
        );
    }

    let invocation = {
        let tunnel = SendPtr(tunnel);
        std::thread::spawn(move || {
            // SAFETY: the test retains the handle and callback context until join.
            unsafe { ft_tunnel_start_connect(tunnel.get(), None, ptr::null_mut()) }
        })
    };
    // SAFETY: blocker remains allocated until both threads join.
    unsafe { &*blocker }.wait_until_entered();

    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (finished_tx, finished_rx) = mpsc::sync_channel(1);
    let replacement = {
        let tunnel = SendPtr(tunnel);
        std::thread::spawn(move || {
            started_tx.send(()).expect("replacement start receiver");
            // SAFETY: the tunnel remains live until this thread joins.
            let status = unsafe { ft_tunnel_set_status_cb(tunnel.get(), None, ptr::null_mut()) };
            finished_tx.send(status).expect("replacement receiver");
        })
    };
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("replacement thread starts");
    let early_result = finished_rx.recv_timeout(Duration::from_millis(100));

    // SAFETY: blocker remains allocated until both threads join.
    unsafe { &*blocker }.release();
    assert_eq!(invocation.join().expect("invocation thread"), FT_OK);
    assert_eq!(replacement.join().expect("replacement thread"), ());
    let replacement_status = match early_result {
        Err(RecvTimeoutError::Timeout) => finished_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("replacement completes after callback"),
        result => panic!("replacement returned during callback: {result:?}"),
    };
    assert_eq!(replacement_status, FT_OK);

    // SAFETY: the callback is cleared and both users of the handle have joined.
    unsafe {
        drop(Box::from_raw(blocker));
        ft_tunnel_release(tunnel);
    }
}

#[test]
fn result_callback_replacement_waits_for_in_flight_invocation() {
    let mut tunnel = ptr::null_mut();
    let mut job = ptr::null_mut();
    let blocker = Box::into_raw(Box::<BlockingInvocation>::default());
    // SAFETY: outputs and callback context are valid until the calls finish.
    unsafe {
        assert_eq!(ft_tunnel_create(ptr::null(), &mut tunnel), FT_OK);
        assert_eq!(ft_job_create(ptr::null(), &mut job), FT_OK);
        assert_eq!(
            ft_job_set_result_cb(
                job,
                Some(blocking_result_callback),
                blocker.cast::<c_void>(),
            ),
            FT_OK
        );
    }

    let invocation = {
        let tunnel = SendPtr(tunnel);
        let job = SendPtr(job);
        std::thread::spawn(move || {
            // SAFETY: the test retains both handles and the callback context until join.
            unsafe { ft_tunnel_start_job(tunnel.get(), job.get()) }
        })
    };
    // SAFETY: blocker remains allocated until both threads join.
    unsafe { &*blocker }.wait_until_entered();

    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (finished_tx, finished_rx) = mpsc::sync_channel(1);
    let replacement = {
        let job = SendPtr(job);
        std::thread::spawn(move || {
            started_tx.send(()).expect("replacement start receiver");
            // SAFETY: the job remains live until this thread joins.
            let status = unsafe { ft_job_set_result_cb(job.get(), None, ptr::null_mut()) };
            finished_tx.send(status).expect("replacement receiver");
        })
    };
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("replacement thread starts");
    let early_result = finished_rx.recv_timeout(Duration::from_millis(100));

    // SAFETY: blocker remains allocated until both threads join.
    unsafe { &*blocker }.release();
    assert_eq!(invocation.join().expect("invocation thread"), FT_OK);
    assert_eq!(replacement.join().expect("replacement thread"), ());
    let replacement_status = match early_result {
        Err(RecvTimeoutError::Timeout) => finished_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("replacement completes after callback"),
        result => panic!("replacement returned during callback: {result:?}"),
    };
    assert_eq!(replacement_status, FT_OK);

    // SAFETY: the callback is cleared and all handle users have joined.
    unsafe {
        drop(Box::from_raw(blocker));
        ft_job_release(job);
        ft_tunnel_release(tunnel);
    }
}

#[test]
fn status_callback_allows_recursive_dispatch() {
    let mut tunnel = ptr::null_mut();
    // SAFETY: output is valid test-local storage.
    unsafe { assert_eq!(ft_tunnel_create(ptr::null(), &mut tunnel), FT_OK) };
    let context = Box::into_raw(Box::new(RecursiveStatusInvocation {
        tunnel: tunnel as usize,
        calls: AtomicUsize::new(0),
        nested_status: AtomicI32::new(FT_EINVAL),
    }));
    // SAFETY: the callback context and tunnel stay live through dispatch and clear.
    unsafe {
        assert_eq!(
            ft_tunnel_set_status_cb(
                tunnel,
                Some(recursive_status_callback),
                context.cast::<c_void>(),
            ),
            FT_OK
        );
    }

    let (finished_tx, finished_rx) = mpsc::sync_channel(1);
    let dispatch = {
        let tunnel = SendPtr(tunnel);
        std::thread::spawn(move || {
            // SAFETY: the test keeps the tunnel and callback context alive until join.
            let status = unsafe { ft_tunnel_start_connect(tunnel.get(), None, ptr::null_mut()) };
            finished_tx.send(status).expect("dispatch receiver");
        })
    };
    assert_eq!(
        finished_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("recursive status dispatch completes"),
        FT_OK
    );
    dispatch.join().expect("dispatch thread");
    // SAFETY: context remains allocated through this read and callback clear.
    let context_ref = unsafe { &*context };
    assert_eq!(context_ref.calls.load(Ordering::SeqCst), 2);
    assert_eq!(context_ref.nested_status.load(Ordering::SeqCst), FT_OK);

    // SAFETY: dispatch has joined; clear retires the context before it is freed.
    unsafe {
        assert_eq!(
            ft_tunnel_set_status_cb(tunnel, None, ptr::null_mut()),
            FT_OK
        );
        drop(Box::from_raw(context));
        ft_tunnel_release(tunnel);
    }
}

#[test]
fn result_callback_allows_recursive_dispatch() {
    let mut tunnel = ptr::null_mut();
    let mut job = ptr::null_mut();
    // SAFETY: outputs are valid test-local storage.
    unsafe {
        assert_eq!(ft_tunnel_create(ptr::null(), &mut tunnel), FT_OK);
        assert_eq!(ft_job_create(ptr::null(), &mut job), FT_OK);
    }
    let context = Box::into_raw(Box::new(RecursiveResultInvocation {
        tunnel: tunnel as usize,
        job: job as usize,
        calls: AtomicUsize::new(0),
        nested_status: AtomicI32::new(FT_EINVAL),
    }));
    // SAFETY: the callback context and handles stay live through dispatch and clear.
    unsafe {
        assert_eq!(
            ft_job_set_result_cb(
                job,
                Some(recursive_result_callback),
                context.cast::<c_void>(),
            ),
            FT_OK
        );
    }

    let (finished_tx, finished_rx) = mpsc::sync_channel(1);
    let dispatch = {
        let tunnel = SendPtr(tunnel);
        let job = SendPtr(job);
        std::thread::spawn(move || {
            // SAFETY: the test keeps both handles and callback context alive until join.
            let status = unsafe { ft_tunnel_start_job(tunnel.get(), job.get()) };
            finished_tx.send(status).expect("dispatch receiver");
        })
    };
    assert_eq!(
        finished_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("recursive result dispatch completes"),
        FT_OK
    );
    dispatch.join().expect("dispatch thread");
    // SAFETY: context remains allocated through this read and callback clear.
    let context_ref = unsafe { &*context };
    assert_eq!(context_ref.calls.load(Ordering::SeqCst), 2);
    assert_eq!(context_ref.nested_status.load(Ordering::SeqCst), FT_OK);

    // SAFETY: dispatch has joined; clear retires the context before it is freed.
    unsafe {
        assert_eq!(ft_job_set_result_cb(job, None, ptr::null_mut()), FT_OK);
        drop(Box::from_raw(context));
        ft_job_release(job);
        ft_tunnel_release(tunnel);
    }
}
