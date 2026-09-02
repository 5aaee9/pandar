use std::ffi::c_void;

use super::*;
use crate::{PluginHttpResult, read_utf8, result};

unsafe fn session(
    session_ptr: *mut c_void,
) -> Option<&'static crate::connection::ConnectionSession> {
    unsafe { crate::connection::ffi::session(session_ptr) }
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_studio_request_snapshot(
    session_ptr: *mut c_void,
    dev_id_ptr: *const u8,
    dev_id_len: usize,
    context: *mut c_void,
    visitor: Option<StudioRequestVisitor>,
) -> PluginStudioRequestState {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return PluginStudioRequestState {
            status: -1,
            authorized: 0,
            account_transition_pending: 0,
            account_epoch: 0,
            cache_generation: 0,
        };
    };
    let Some(dev_id) = (unsafe { read_utf8(dev_id_ptr, dev_id_len) }) else {
        return PluginStudioRequestState {
            status: -19,
            authorized: 0,
            account_transition_pending: 0,
            account_epoch: 0,
            cache_generation: 0,
        };
    };
    let (result, snapshot) = session.studio_request_snapshot(dev_id);
    if let Some(visitor) = visitor {
        visitor(
            context,
            snapshot.hub_url.as_ptr(),
            snapshot.hub_url.len(),
            snapshot.token.as_ptr(),
            snapshot.token.len(),
            snapshot.printer_id.as_ptr(),
            snapshot.printer_id.len(),
        );
    }
    result
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_studio_set_listener(
    session_ptr: *mut c_void,
    kind: i32,
    present: bool,
) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return -1;
    };
    if session.studio_set_listener(kind, present) {
        0
    } else {
        -19
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_studio_selected(
    session_ptr: *mut c_void,
) -> PluginHttpResult {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return result(-1, 0, String::new());
    };
    result(0, 200, session.studio_selected())
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_studio_set_selected(
    session_ptr: *mut c_void,
    selected_ptr: *const u8,
    selected_len: usize,
) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return -1;
    };
    let Some(selected) = (unsafe { read_utf8(selected_ptr, selected_len) }) else {
        return -19;
    };
    session.studio_set_selected(selected);
    0
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_studio_add_subscription(
    session_ptr: *mut c_void,
    dev_id_ptr: *const u8,
    dev_id_len: usize,
) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return -1;
    };
    let Some(dev_id) =
        unsafe { read_utf8(dev_id_ptr, dev_id_len) }.filter(|dev_id| !dev_id.is_empty())
    else {
        return -2;
    };
    session.studio_add_subscription(dev_id);
    0
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_studio_del_subscription(
    session_ptr: *mut c_void,
    dev_id_ptr: *const u8,
    dev_id_len: usize,
) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return -1;
    };
    let Some(dev_id) =
        unsafe { read_utf8(dev_id_ptr, dev_id_len) }.filter(|dev_id| !dev_id.is_empty())
    else {
        return -2;
    };
    session.studio_del_subscription(dev_id);
    0
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_studio_heartbeat_plan(
    session_ptr: *mut c_void,
    context: *mut c_void,
    visitor: Option<StudioHeartbeatVisitor>,
) -> PluginStudioHeartbeatPlan {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return PluginStudioHeartbeatPlan {
            wait_ms: DISPATCHER_IDLE_WAIT_MS,
            refresh: 0,
        };
    };
    let (plan, targets) = session.studio_heartbeat_plan();
    if let Some(visitor) = visitor {
        for target in targets {
            visitor(
                context,
                target.tunnel,
                target.dev_id.as_ptr(),
                target.dev_id.len(),
                target.generation,
            );
        }
    }
    plan
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_studio_disconnect_local(session_ptr: *mut c_void) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return -1;
    };
    session.studio_disconnect_local();
    0
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_studio_local_generation(
    session_ptr: *mut c_void,
    dev_id_ptr: *const u8,
    dev_id_len: usize,
) -> u64 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return 0;
    };
    unsafe { read_utf8(dev_id_ptr, dev_id_len) }
        .map(|dev_id| session.studio_local_generation(dev_id))
        .unwrap_or_default()
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_studio_take_work(
    session_ptr: *mut c_void,
    context: *mut c_void,
    visitor: Option<StudioWorkVisitor>,
) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return -1;
    };
    let work = session.studio_take_work();
    if let Some(visitor) = visitor {
        for item in work {
            visitor(
                context,
                item.kind,
                item.state,
                item.ticket,
                item.generation,
                item.dev_id.as_ptr(),
                item.dev_id.len(),
                item.body.as_ptr(),
                item.body.len(),
            );
        }
    }
    0
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_studio_begin_account_transition(
    session_ptr: *mut c_void,
) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return -1;
    };
    session.begin_account_transition();
    0
}

#[unsafe(no_mangle)]
/// # Safety
/// Handles must be live, byte inputs valid for paired lengths, outputs writable, and callback contexts valid for the call.
pub unsafe extern "C" fn pandar_plugin_studio_finish_account_transition(
    session_ptr: *mut c_void,
    account_epoch: u64,
) -> i32 {
    let Some(session) = (unsafe { session(session_ptr) }) else {
        return -1;
    };
    session.finish_account_transition(account_epoch);
    0
}
