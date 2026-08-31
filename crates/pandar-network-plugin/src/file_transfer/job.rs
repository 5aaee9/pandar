use std::ffi::{c_char, c_int, c_void};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use super::*;

struct FtJob {
    refs: AtomicUsize,
    result_cb: Mutex<Option<Callback<FtJobResultCb>>>,
    msg_cb: Mutex<Option<Callback<FtJobMsgCb>>>,
    cancelled: AtomicBool,
    state: Mutex<FtJobState>,
    finished: Condvar,
}

struct FtJobState {
    finished: bool,
    result: FtJobResult,
}

impl Refcounted for FtJob {
    fn refs(&self) -> &AtomicUsize {
        &self.refs
    }
}

// SAFETY for all exported functions below: handles originate from ft_job_create
// and remain valid while their refcount is non-zero; callers follow the
// retain/release protocol of the Studio ABI.

#[unsafe(no_mangle)]
/// # Safety
/// `out` must be null or writable for one job-handle pointer. On success it receives one owned
/// reference that must eventually be consumed exactly once by `ft_job_release`.
pub unsafe extern "C" fn ft_job_create(
    _config_json: *const c_char,
    out: *mut *mut FtJobHandle,
) -> c_int {
    if out.is_null() {
        return FT_EINVAL;
    }
    let job = Box::new(FtJob {
        refs: AtomicUsize::new(1),
        result_cb: Mutex::new(None),
        msg_cb: Mutex::new(None),
        cancelled: AtomicBool::new(false),
        state: Mutex::new(FtJobState {
            finished: false,
            result: FtJobResult::error(FT_OK),
        }),
        finished: Condvar::new(),
    });
    // SAFETY: out is non-null (checked above).
    unsafe { *out = Box::into_raw(job) as *mut FtJobHandle };
    FT_OK
}

#[unsafe(no_mangle)]
/// # Safety
/// `handle` must own one live job reference. This adds another owned reference that must later be
/// consumed by exactly one `ft_job_release` call.
pub unsafe extern "C" fn ft_job_retain(handle: *mut FtJobHandle) {
    if !handle.is_null() {
        // SAFETY: see module-level note.
        unsafe { retain(handle as *const FtJob) };
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `handle` must own one live job reference. This consumes that reference and may destroy the
/// allocation; the caller must not use that reference after this call.
pub unsafe extern "C" fn ft_job_release(handle: *mut FtJobHandle) {
    // SAFETY: see module-level note.
    unsafe { release(handle as *mut FtJob) };
}

#[unsafe(no_mangle)]
/// # Safety
/// `handle` must identify a live job. When `cb` is present, it and `user` must remain valid and
/// safe to invoke until replaced, cleared with `None`, or the final job reference is released.
pub unsafe extern "C" fn ft_job_set_result_cb(
    handle: *mut FtJobHandle,
    cb: Option<FtJobResultCb>,
    user: *mut c_void,
) -> c_int {
    if handle.is_null() {
        return FT_EINVAL;
    }
    // SAFETY: see module-level note.
    *unsafe { &*(handle as *const FtJob) }
        .result_cb
        .lock()
        .expect("job result callback mutex poisoned") =
        cb.map(|function| Callback { function, user });
    FT_OK
}

#[unsafe(no_mangle)]
/// # Safety
/// `handle` must identify a live job reference for the full call and `out` must be writable for one
/// `FtJobResult`. The handle must not be released concurrently while this call is waiting.
pub unsafe extern "C" fn ft_job_get_result(
    handle: *mut FtJobHandle,
    timeout_ms: u32,
    out: *mut FtJobResult,
) -> c_int {
    if handle.is_null() || out.is_null() {
        return FT_EINVAL;
    }
    // SAFETY: see module-level note; out is non-null (checked above).
    let job = unsafe { &*(handle as *const FtJob) };
    let mut state = job.state.lock().expect("job state mutex poisoned");
    let (guard, _) = job
        .finished
        .wait_timeout_while(
            state,
            Duration::from_millis(u64::from(timeout_ms)),
            |state| !state.finished,
        )
        .expect("job state mutex poisoned");
    state = guard;
    // SAFETY: out is non-null (checked above).
    unsafe {
        *out = if state.finished {
            FtJobResult::error(state.result.ec)
        } else {
            FtJobResult::error(FT_ETIMEOUT)
        };
    }
    FT_OK
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn ft_tunnel_start_job(
    tunnel: *mut FtTunnelHandle,
    job: *mut FtJobHandle,
) -> c_int {
    if tunnel.is_null() || job.is_null() {
        return FT_EINVAL;
    }
    // SAFETY: see module-level note.
    let job = unsafe { &*(job as *const FtJob) };
    {
        let mut state = job.state.lock().expect("job state mutex poisoned");
        state.result = FtJobResult::error(FT_EIO);
        state.finished = true;
    }
    job.finished.notify_all();
    let result_cb = job
        .result_cb
        .lock()
        .expect("job result callback mutex poisoned")
        .as_ref()
        .map(|callback| (callback.function, callback.user));
    if let Some((result_cb, result_user)) = result_cb {
        result_cb(result_user, FtJobResult::error(FT_EIO));
    }
    FT_OK
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn ft_job_cancel(handle: *mut FtJobHandle) -> c_int {
    if handle.is_null() {
        return FT_EINVAL;
    }
    // SAFETY: see module-level note.
    unsafe { &*(handle as *const FtJob) }
        .cancelled
        .store(true, Ordering::Relaxed);
    FT_OK
}

#[unsafe(no_mangle)]
/// # Safety
/// `handle` must identify a live job. When `cb` is present, it and `user` must remain valid and
/// safe to invoke until replaced, cleared with `None`, or the final job reference is released.
pub unsafe extern "C" fn ft_job_set_msg_cb(
    handle: *mut FtJobHandle,
    cb: Option<FtJobMsgCb>,
    user: *mut c_void,
) -> c_int {
    if handle.is_null() {
        return FT_EINVAL;
    }
    // SAFETY: see module-level note.
    *unsafe { &*(handle as *const FtJob) }
        .msg_cb
        .lock()
        .expect("job message callback mutex poisoned") =
        cb.map(|function| Callback { function, user });
    FT_OK
}

#[unsafe(no_mangle)]
/// # Safety
/// Any non-null `handle` must identify a live job reference and any non-null `out` must be writable
/// for one `FtJobMsg` for the duration of this call.
pub unsafe extern "C" fn ft_job_try_get_msg(handle: *mut FtJobHandle, out: *mut FtJobMsg) -> c_int {
    if !out.is_null() {
        // SAFETY: out is non-null (checked above).
        unsafe {
            *out = FtJobMsg {
                kind: 0,
                json: std::ptr::null(),
            }
        };
    }
    if handle.is_null() { FT_EINVAL } else { FT_EIO }
}

#[unsafe(no_mangle)]
/// # Safety
/// Any non-null `handle` must identify a live job reference and any non-null `out` must be writable
/// for one `FtJobMsg` for the duration of this call.
pub unsafe extern "C" fn ft_job_get_msg(
    handle: *mut FtJobHandle,
    _timeout_ms: u32,
    out: *mut FtJobMsg,
) -> c_int {
    if !out.is_null() {
        // SAFETY: out is non-null (checked above).
        unsafe {
            *out = FtJobMsg {
                kind: 0,
                json: std::ptr::null(),
            }
        };
    }
    if handle.is_null() { FT_EINVAL } else { FT_EIO }
}

#[cfg(test)]
mod tests {
    use std::ptr;
    use std::time::{Duration, Instant};

    use super::*;

    struct SendPtr<T>(*mut T);
    // SAFETY: FtJob state is mutex/atomic owned and the Studio ABI shares job
    // handles across threads by design.
    unsafe impl<T> Send for SendPtr<T> {}

    impl<T> SendPtr<T> {
        fn get(&self) -> *mut T {
            self.0
        }
    }

    #[test]
    fn get_result_waits_for_completion_and_reports_the_job_ec() {
        let mut job: *mut FtJobHandle = ptr::null_mut();
        let mut tunnel: *mut FtTunnelHandle = ptr::null_mut();
        // SAFETY: out pointers are valid test-local storage.
        unsafe {
            assert_eq!(ft_job_create(ptr::null(), &mut job), FT_OK);
            assert_eq!(ft_tunnel_create(ptr::null(), &mut tunnel), FT_OK);
        }
        let out = Box::into_raw(Box::new(FtJobResult::error(FT_OK)));

        let completer = {
            let job = SendPtr(job);
            let tunnel = SendPtr(tunnel);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                // Both handles stay alive until joined below.
                unsafe { ft_tunnel_start_job(tunnel.get(), job.get()) }
            })
        };

        let SendPtr(job) = SendPtr(job);
        // SAFETY: out is valid storage for one FtJobResult for the whole call.
        let status = unsafe { ft_job_get_result(job, 10_000, out) };
        assert_eq!(status, FT_OK);
        // SAFETY: start_job completed before the join above returned.
        let result_ec = unsafe { (*out).ec };
        assert_eq!(result_ec, FT_EIO);
        assert_eq!(completer.join().expect("completer thread"), FT_OK);

        // SAFETY: each handle owns its last reference.
        unsafe {
            ft_job_release(job);
            ft_tunnel_release(tunnel);
            drop(Box::from_raw(out));
        }
    }

    #[test]
    fn get_result_times_out_when_the_job_never_finishes() {
        let mut job: *mut FtJobHandle = ptr::null_mut();
        // SAFETY: out pointer is valid test-local storage.
        unsafe { assert_eq!(ft_job_create(ptr::null(), &mut job), FT_OK) };
        let out = Box::into_raw(Box::new(FtJobResult::error(FT_OK)));

        let started = Instant::now();
        // SAFETY: the job handle stays valid until released below.
        let status = unsafe { ft_job_get_result(job, 50, out) };
        let elapsed = started.elapsed();

        assert_eq!(status, FT_OK);
        // SAFETY: out is valid storage written by the call above.
        assert_eq!(unsafe { (*out).ec }, FT_ETIMEOUT);
        assert!(
            elapsed >= Duration::from_millis(40),
            "waited only {elapsed:?}"
        );

        let SendPtr(job) = SendPtr(job);
        // SAFETY: the job handle owns its last reference.
        unsafe {
            ft_job_release(job);
            drop(Box::from_raw(out));
        }
    }
}
