//! Studio file-transfer ABI (`ft_*`), fail-closed.
//!
//! Bambu Studio resolves these symbols directly from the plugin library, so the
//! implementations live in Rust and the C++ shim does not adapt them. Every
//! operation reports the same stable `unsupported_file_transfer` error.

use std::ffi::{c_char, c_int, c_void};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

const FT_OK: c_int = 0;
const FT_EINVAL: c_int = -1;
const FT_EIO: c_int = -3;
const FT_ETIMEOUT: c_int = -4;

#[repr(C)]
pub struct FtJobResult {
    pub ec: c_int,
    pub resp_ec: c_int,
    pub json: *const c_char,
    pub bin: *const c_void,
    pub bin_size: u32,
}

#[repr(C)]
pub struct FtJobMsg {
    pub kind: c_int,
    pub json: *const c_char,
}

type FtTunnelConnectCb = extern "C" fn(*mut c_void, c_int, c_int, *const c_char);
type FtTunnelStatusCb = extern "C" fn(*mut c_void, c_int, c_int, c_int, *const c_char);
type FtJobResultCb = extern "C" fn(*mut c_void, FtJobResult);
type FtJobMsgCb = extern "C" fn(*mut c_void, FtJobMsg);

pub enum FtTunnelHandle {}
pub enum FtJobHandle {}

struct Callback<F> {
    function: F,
    user: *mut c_void,
}

// The Studio ABI passes opaque user pointers across threads by design; the
// callback itself is responsible for the pointed-to data.
unsafe impl<F> Send for Callback<F> {}
unsafe impl<F> Sync for Callback<F> {}

struct CallbackSlot<F> {
    current: Mutex<Option<Arc<CallbackRegistration<F>>>>,
}

struct CallbackRegistration<F> {
    callback: Callback<F>,
    active: Mutex<usize>,
    quiescent: Condvar,
}

struct CallbackLease<F> {
    registration: Arc<CallbackRegistration<F>>,
}

impl<F> CallbackSlot<F> {
    fn new() -> Self {
        Self {
            current: Mutex::new(None),
        }
    }

    fn replace(&self, callback: Option<Callback<F>>) {
        let replacement = callback.map(|callback| {
            Arc::new(CallbackRegistration {
                callback,
                active: Mutex::new(0),
                quiescent: Condvar::new(),
            })
        });
        let previous = {
            let mut current = self
                .current
                .lock()
                .expect("callback registration mutex poisoned");
            std::mem::replace(&mut *current, replacement)
        };
        if let Some(previous) = previous {
            let active = previous
                .active
                .lock()
                .expect("callback activity mutex poisoned");
            drop(
                previous
                    .quiescent
                    .wait_while(active, |active| *active != 0)
                    .expect("callback activity mutex poisoned"),
            );
        }
    }

    fn acquire(&self) -> Option<CallbackLease<F>> {
        let registration = {
            let current = self
                .current
                .lock()
                .expect("callback registration mutex poisoned");
            let registration = Arc::clone(current.as_ref()?);
            let mut active = registration
                .active
                .lock()
                .expect("callback activity mutex poisoned");
            *active += 1;
            drop(active);
            registration
        };
        Some(CallbackLease { registration })
    }
}

impl<F> CallbackLease<F> {
    fn callback(&self) -> &Callback<F> {
        &self.registration.callback
    }
}

impl<F> Drop for CallbackLease<F> {
    fn drop(&mut self) {
        let mut active = self
            .registration
            .active
            .lock()
            .expect("callback activity mutex poisoned");
        *active -= 1;
        if *active == 0 {
            self.registration.quiescent.notify_all();
        }
    }
}

struct FtTunnel {
    refs: AtomicUsize,
    status_cb: CallbackSlot<FtTunnelStatusCb>,
    closed: AtomicBool,
}

mod job;
#[cfg(test)]
mod tests;

impl FtJobResult {
    const fn error(ec: c_int) -> Self {
        Self {
            ec,
            resp_ec: 0,
            json: std::ptr::null(),
            bin: std::ptr::null(),
            bin_size: 0,
        }
    }
}

fn unavailable_message() -> std::ffi::CString {
    std::ffi::CString::new(crate::stable_error_body("unsupported_file_transfer"))
        .expect("stable error body contains no NUL")
}

unsafe fn retain<T: Refcounted>(handle: *const T) {
    debug_assert!(!handle.is_null());
    // SAFETY: handle comes from ft_*_create and stays alive while refs > 0.
    unsafe { &*handle }.refs().fetch_add(1, Ordering::Relaxed);
}

unsafe fn release<T: Refcounted>(handle: *mut T) {
    if handle.is_null() {
        return;
    }
    // SAFETY: see retain; the last release reclaims the allocation.
    if unsafe { &*handle }.refs().fetch_sub(1, Ordering::AcqRel) == 1 {
        drop(unsafe { Box::from_raw(handle) });
    }
}

trait Refcounted {
    fn refs(&self) -> &AtomicUsize;
}

impl Refcounted for FtTunnel {
    fn refs(&self) -> &AtomicUsize {
        &self.refs
    }
}

// SAFETY for all exported functions below: handles originate from
// ft_tunnel_create / ft_job_create and remain valid while their refcount is
// non-zero; callers follow the retain/release protocol of the Studio ABI.

#[unsafe(no_mangle)]
pub extern "C" fn ft_abi_version() -> c_int {
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn ft_free(_ptr: *mut c_void) {}

#[unsafe(no_mangle)]
pub extern "C" fn ft_job_result_destroy(_result: *mut FtJobResult) {}

#[unsafe(no_mangle)]
pub extern "C" fn ft_job_msg_destroy(_msg: *mut FtJobMsg) {}

/// Creates a file-transfer tunnel handle for the Studio ABI.
///
/// # Safety
///
/// `out` must be null or point to writable storage for one tunnel-handle pointer. On
/// success, the caller owns the returned reference and must release it with
/// [`ft_tunnel_release`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ft_tunnel_create(
    _config_json: *const c_char,
    out: *mut *mut FtTunnelHandle,
) -> c_int {
    if out.is_null() {
        return FT_EINVAL;
    }
    let tunnel = Box::new(FtTunnel {
        refs: AtomicUsize::new(1),
        status_cb: CallbackSlot::new(),
        closed: AtomicBool::new(false),
    });
    // SAFETY: out is non-null (checked above).
    unsafe { *out = Box::into_raw(tunnel) as *mut FtTunnelHandle };
    FT_OK
}

#[unsafe(no_mangle)]
/// # Safety
/// `handle` must own one live tunnel reference. This adds another owned reference that must later
/// be consumed by exactly one `ft_tunnel_release` call.
pub unsafe extern "C" fn ft_tunnel_retain(handle: *mut FtTunnelHandle) {
    if !handle.is_null() {
        // SAFETY: see module-level note.
        unsafe { retain(handle as *const FtTunnel) };
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `handle` must own one live tunnel reference. This consumes that reference and may destroy the
/// allocation; the caller must not use that reference after this call.
pub unsafe extern "C" fn ft_tunnel_release(handle: *mut FtTunnelHandle) {
    // SAFETY: see module-level note.
    unsafe { release(handle as *mut FtTunnel) };
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn ft_tunnel_start_connect(
    handle: *mut FtTunnelHandle,
    cb: Option<FtTunnelConnectCb>,
    user: *mut c_void,
) -> c_int {
    if handle.is_null() {
        return FT_EINVAL;
    }
    let tunnel = handle as *const FtTunnel;
    // SAFETY: see module-level note.
    unsafe { retain(tunnel) };
    let message = unavailable_message();
    if let Some(cb) = cb {
        cb(user, 1, FT_EIO, message.as_ptr());
    }
    // SAFETY: tunnel is retained for the duration of this call.
    if let Some(status_cb) = unsafe { &*tunnel }.status_cb.acquire() {
        let callback = status_cb.callback();
        (callback.function)(callback.user, 0, -1, FT_EIO, message.as_ptr());
    }
    // SAFETY: balances the retain above.
    unsafe { release(tunnel as *mut FtTunnel) };
    FT_OK
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn ft_tunnel_sync_connect(handle: *mut FtTunnelHandle) -> c_int {
    if handle.is_null() { FT_EINVAL } else { FT_EIO }
}

#[unsafe(no_mangle)]
/// Replacing or clearing the callback waits for any invocation of the previous callback to return.
/// A newly registered callback may run before this function returns. A status callback must not
/// synchronously call or wait for this setter to replace its own registration.
///
/// # Safety
/// `handle` must identify a live tunnel. When `cb` is present, it and `user` must remain valid and
/// safe to invoke until replaced, cleared with `None`, or the final tunnel reference is released.
/// Once this function returns, a replaced callback and its context are no longer in flight.
pub unsafe extern "C" fn ft_tunnel_set_status_cb(
    handle: *mut FtTunnelHandle,
    cb: Option<FtTunnelStatusCb>,
    user: *mut c_void,
) -> c_int {
    if handle.is_null() {
        return FT_EINVAL;
    }
    // SAFETY: see module-level note.
    unsafe { &*(handle as *const FtTunnel) }
        .status_cb
        .replace(cb.map(|function| Callback { function, user }));
    FT_OK
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn ft_tunnel_shutdown(handle: *mut FtTunnelHandle) -> c_int {
    if handle.is_null() {
        return FT_EINVAL;
    }
    // SAFETY: see module-level note.
    unsafe { &*(handle as *const FtTunnel) }
        .closed
        .store(true, Ordering::Relaxed);
    FT_OK
}
