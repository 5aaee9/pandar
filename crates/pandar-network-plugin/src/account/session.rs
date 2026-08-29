use std::{collections::VecDeque, ffi::c_void, sync::Mutex};

use super::lifecycle::{
    PluginLifecycleResult,
    transaction::{AccountView, PluginAccountBytes, PluginAccountMutation},
};

mod callbacks;
mod mutation;
#[cfg(test)]
mod tests;

use mutation::apply_mutation;
#[cfg(test)]
use mutation::{
    MUTATION_CLEAR, MUTATION_FIRMWARE_FENCE, MUTATION_HTTP_ERROR, MUTATION_LOGIN, MUTATION_REPLACE,
    MUTATION_RESTORE_FAILURE, MUTATION_RUNTIME_HUB,
};

#[repr(C)]
pub struct PluginAccountSessionBridge {
    pub replace: extern "C" fn(
        *mut c_void,
        PluginAccountBytes,
        PluginAccountBytes,
        PluginAccountBytes,
        PluginAccountBytes,
        PluginAccountBytes,
        PluginAccountBytes,
        i32,
    ),
    pub clear: extern "C" fn(*mut c_void),
    pub set_hub_url: extern "C" fn(*mut c_void, PluginAccountBytes),
    pub invoke_user_login: extern "C" fn(*mut c_void, i32, bool),
    pub invoke_http_error: extern "C" fn(*mut c_void, u32, PluginAccountBytes),
    pub reset_personal_presets: extern "C" fn(*mut c_void),
}

pub struct AccountLifecycleSession {
    callbacks: Mutex<CallbackQueue>,
}

#[derive(Default)]
struct CallbackQueue {
    pending: VecDeque<AccountCallback>,
    draining: bool,
}

struct TransitionCallback {
    account_epoch: u64,
    notification: Option<bool>,
    expected: Option<ExpectedAccount>,
    error: Option<(u32, String)>,
}

enum AccountCallback {
    Transition(TransitionCallback),
    UserLogin(bool),
    HttpError(u32, String),
}

#[derive(Clone)]
struct ExpectedAccount {
    hub_url: String,
    token: String,
    account_epoch: u64,
    config_epoch: u64,
    session_kind: i32,
}

impl AccountLifecycleSession {
    fn new() -> Self {
        Self {
            callbacks: Mutex::new(CallbackQueue::default()),
        }
    }

    fn enqueue(&self, callback: AccountCallback) {
        self.callbacks
            .lock()
            .expect("account callback queue poisoned")
            .pending
            .push_back(callback);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_account_session_create() -> *mut c_void {
    Box::into_raw(Box::new(AccountLifecycleSession::new())).cast()
}

#[unsafe(no_mangle)]
/// # Safety
/// `session` and `result` must be live for the call. The HTTP body is borrowed for the call.
pub unsafe extern "C" fn pandar_plugin_account_session_apply_lifecycle_result(
    session: *mut c_void,
    lifecycle: *const PluginLifecycleResult,
) {
    let (Some(session), Some(lifecycle)) = (
        unsafe { session.cast::<AccountLifecycleSession>().as_ref() },
        unsafe { lifecycle.as_ref() },
    ) else {
        return;
    };
    if lifecycle.account_event == 1 {
        session.enqueue(AccountCallback::UserLogin(true));
    } else if lifecycle.report_http_error != 0 {
        let body = match (PluginAccountBytes {
            ptr: lifecycle.http.body_ptr,
            len: lifecycle.http.body_len,
        })
        .read("account lifecycle HTTP error body")
        {
            Ok(body) => body,
            Err(error) => {
                eprintln!("pandar account lifecycle callback failed: {error:#}");
                return;
            }
        };
        session.enqueue(AccountCallback::HttpError(lifecycle.http.http_code, body));
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// `session` must be null or returned by account session creation exactly once.
pub unsafe extern "C" fn pandar_plugin_account_session_destroy(session: *mut c_void) {
    if !session.is_null() {
        drop(unsafe { Box::from_raw(session.cast::<AccountLifecycleSession>()) });
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// All opaque pointers and bridges must remain live for the call. Mutation byte fields are borrowed
/// for the call and must be valid for their lengths.
pub unsafe extern "C" fn pandar_plugin_account_session_apply(
    session_ptr: *mut c_void,
    connection_ptr: *mut c_void,
    firmware_ptr: *mut c_void,
    bridge_ptr: *const PluginAccountSessionBridge,
    agent: *mut c_void,
    current_ptr: *const crate::account::lifecycle::transaction::PluginAccountView,
    mutation_ptr: *const PluginAccountMutation,
) -> i32 {
    let Some(session) = (unsafe { session_ptr.cast::<AccountLifecycleSession>().as_ref() }) else {
        return 1;
    };
    let (Some(bridge), Some(mutation)) = (unsafe { bridge_ptr.as_ref() }, unsafe {
        mutation_ptr.as_ref()
    }) else {
        return 1;
    };
    let current = match AccountView::read(current_ptr) {
        Ok(current) => current,
        Err(error) => {
            eprintln!("pandar account mutation application failed: {error:#}");
            return 1;
        }
    };
    if let Err(error) = apply_mutation(
        session,
        connection_ptr,
        firmware_ptr,
        bridge,
        agent,
        &current,
        mutation,
    ) {
        eprintln!("pandar account mutation application failed: {error:#}");
        return 1;
    }
    0
}
