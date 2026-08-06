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
pub extern "C" fn ft_job_retain(handle: *mut FtJobHandle) {
    if !handle.is_null() {
        // SAFETY: see module-level note.
        unsafe { retain(handle as *const FtJob) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ft_job_release(handle: *mut FtJobHandle) {
    // SAFETY: see module-level note.
    unsafe { release(handle as *mut FtJob) };
}

#[unsafe(no_mangle)]
pub extern "C" fn ft_job_set_result_cb(
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
    if !state.finished {
        let (guard, _) = job
            .finished
            .wait_timeout_while(
                state,
                Duration::from_millis(u64::from(timeout_ms)),
                |state| state.finished,
            )
            .expect("job state mutex poisoned");
        state = guard;
    }
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
pub extern "C" fn ft_tunnel_start_job(tunnel: *mut FtTunnelHandle, job: *mut FtJobHandle) -> c_int {
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
pub extern "C" fn ft_job_cancel(handle: *mut FtJobHandle) -> c_int {
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
pub extern "C" fn ft_job_set_msg_cb(
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
