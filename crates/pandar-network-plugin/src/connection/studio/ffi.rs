use std::ffi::c_void;

use super::*;
use crate::{PluginHttpResult, read_utf8, result};

fn session(session_ptr: *mut c_void) -> Option<&'static crate::connection::ConnectionSession> {
    crate::connection::ffi::session(session_ptr)
}

fn invalid_delivery(status: i32) -> PluginStudioDeliveryResult {
    PluginStudioDeliveryResult {
        status,
        ticket: 0,
        local_generation: 0,
        account_epoch: 0,
        cache_generation: 0,
    }
}

fn visit_payload(
    payload: Option<StudioPayload>,
    context: *mut c_void,
    visitor: Option<StudioPayloadVisitor>,
) {
    let (Some(payload), Some(visitor)) = (payload, visitor) else {
        return;
    };
    visitor(
        context,
        payload.dev_id.as_ptr(),
        payload.dev_id.len(),
        payload.body.as_ptr(),
        payload.body.len(),
        payload.printer_id.as_ptr(),
        payload.printer_id.len(),
        payload.model.as_ptr(),
        payload.model.len(),
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_request_snapshot(
    session_ptr: *mut c_void,
    dev_id_ptr: *const u8,
    dev_id_len: usize,
    context: *mut c_void,
    visitor: Option<StudioRequestVisitor>,
) -> PluginStudioRequestState {
    let Some(session) = session(session_ptr) else {
        return PluginStudioRequestState {
            status: -1,
            authorized: 0,
            account_transition_pending: 0,
            account_epoch: 0,
            cache_generation: 0,
        };
    };
    let Some(dev_id) = read_utf8(dev_id_ptr, dev_id_len) else {
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
pub extern "C" fn pandar_plugin_connection_studio_snapshot_current(
    session_ptr: *mut c_void,
    account_epoch: u64,
    cache_generation: u64,
) -> i32 {
    session(session_ptr).is_some_and(|session| {
        session.studio_request_snapshot_current(account_epoch, cache_generation)
    }) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_account_request_admitted(session_ptr: *mut c_void) -> i32 {
    session(session_ptr).is_some_and(|session| session.studio_account_request_admitted()) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_account_request_current(
    session_ptr: *mut c_void,
    account_epoch: u64,
) -> i32 {
    session(session_ptr)
        .is_some_and(|session| session.studio_account_request_current(account_epoch)) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_set_listener(
    session_ptr: *mut c_void,
    kind: i32,
    present: bool,
) -> i32 {
    let Some(session) = session(session_ptr) else {
        return -1;
    };
    if session.studio_set_listener(kind, present) {
        0
    } else {
        -19
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_selected(session_ptr: *mut c_void) -> PluginHttpResult {
    let Some(session) = session(session_ptr) else {
        return result(-1, 0, String::new());
    };
    result(0, 200, session.studio_selected())
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_set_selected(
    session_ptr: *mut c_void,
    selected_ptr: *const u8,
    selected_len: usize,
) -> i32 {
    let Some(session) = session(session_ptr) else {
        return -1;
    };
    let Some(selected) = read_utf8(selected_ptr, selected_len) else {
        return -19;
    };
    session.studio_set_selected(selected);
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_add_subscription(
    session_ptr: *mut c_void,
    dev_id_ptr: *const u8,
    dev_id_len: usize,
) -> i32 {
    let Some(session) = session(session_ptr) else {
        return -1;
    };
    let Some(dev_id) = read_utf8(dev_id_ptr, dev_id_len).filter(|dev_id| !dev_id.is_empty()) else {
        return -2;
    };
    session.studio_add_subscription(dev_id);
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_del_subscription(
    session_ptr: *mut c_void,
    dev_id_ptr: *const u8,
    dev_id_len: usize,
) -> i32 {
    let Some(session) = session(session_ptr) else {
        return -1;
    };
    let Some(dev_id) = read_utf8(dev_id_ptr, dev_id_len).filter(|dev_id| !dev_id.is_empty()) else {
        return -2;
    };
    session.studio_del_subscription(dev_id);
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_heartbeat_plan(
    session_ptr: *mut c_void,
    context: *mut c_void,
    visitor: Option<StudioHeartbeatVisitor>,
) -> PluginStudioHeartbeatPlan {
    let Some(session) = session(session_ptr) else {
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
pub extern "C" fn pandar_plugin_studio_prepare_connected(
    session_ptr: *mut c_void,
    dev_id_ptr: *const u8,
    dev_id_len: usize,
    now_ms: u64,
    context: *mut c_void,
    visitor: Option<StudioPayloadVisitor>,
) -> PluginStudioDeliveryResult {
    let Some(session) = session(session_ptr) else {
        return invalid_delivery(-1);
    };
    let Some(dev_id) = read_utf8(dev_id_ptr, dev_id_len) else {
        return invalid_delivery(-2);
    };
    let (delivery, payload) = session.studio_prepare_connected(dev_id, now_ms);
    visit_payload(payload, context, visitor);
    delivery
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_prepare_message(
    session_ptr: *mut c_void,
    tunnel: i32,
    dev_id_ptr: *const u8,
    dev_id_len: usize,
    local_generation: u64,
    initialize_cloud: bool,
    expected_cache_generation: u64,
    context: *mut c_void,
    visitor: Option<StudioPayloadVisitor>,
) -> PluginStudioDeliveryResult {
    let Some(session) = session(session_ptr) else {
        return invalid_delivery(-1);
    };
    let Some(dev_id) = read_utf8(dev_id_ptr, dev_id_len) else {
        return invalid_delivery(-2);
    };
    let (delivery, payload) = session.studio_prepare_message(
        tunnel,
        dev_id,
        local_generation,
        initialize_cloud,
        expected_cache_generation,
    );
    visit_payload(payload, context, visitor);
    delivery
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_status_target_available(
    session_ptr: *mut c_void,
    tunnel: i32,
    dev_id_ptr: *const u8,
    dev_id_len: usize,
    local_generation: u64,
) -> i32 {
    let Some(session) = session(session_ptr) else {
        return 0;
    };
    read_utf8(dev_id_ptr, dev_id_len).is_some_and(|dev_id| {
        session.studio_status_target_available(tunnel, dev_id, local_generation)
    }) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_connect_local(
    session_ptr: *mut c_void,
    dev_id_ptr: *const u8,
    dev_id_len: usize,
    context: *mut c_void,
    visitor: Option<StudioPayloadVisitor>,
) -> PluginStudioDeliveryResult {
    let Some(session) = session(session_ptr) else {
        return invalid_delivery(-1);
    };
    let Some(dev_id) = read_utf8(dev_id_ptr, dev_id_len) else {
        return invalid_delivery(-2);
    };
    let (delivery, payload) = session.studio_connect_local(dev_id);
    visit_payload(payload, context, visitor);
    delivery
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_disconnect_local(session_ptr: *mut c_void) -> i32 {
    let Some(session) = session(session_ptr) else {
        return -1;
    };
    session.studio_disconnect_local();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_local_generation(
    session_ptr: *mut c_void,
    dev_id_ptr: *const u8,
    dev_id_len: usize,
) -> u64 {
    let Some(session) = session(session_ptr) else {
        return 0;
    };
    read_utf8(dev_id_ptr, dev_id_len)
        .map(|dev_id| session.studio_local_generation(dev_id))
        .unwrap_or_default()
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_complete_delivery(
    session_ptr: *mut c_void,
    ticket: u64,
    delivered: bool,
) -> i32 {
    session(session_ptr).is_some_and(|session| session.studio_complete_delivery(ticket, delivered))
        as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_claim_delivery(
    session_ptr: *mut c_void,
    ticket: u64,
) -> i32 {
    session(session_ptr).is_some_and(|session| session.studio_claim_delivery(ticket)) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_take_work(
    session_ptr: *mut c_void,
    context: *mut c_void,
    visitor: Option<StudioWorkVisitor>,
) -> i32 {
    let Some(session) = session(session_ptr) else {
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
pub extern "C" fn pandar_plugin_studio_begin_account_transition(session_ptr: *mut c_void) -> i32 {
    let Some(session) = session(session_ptr) else {
        return -1;
    };
    session.begin_account_transition();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_studio_finish_account_transition(
    session_ptr: *mut c_void,
    account_epoch: u64,
) -> i32 {
    let Some(session) = session(session_ptr) else {
        return -1;
    };
    session.finish_account_transition(account_epoch);
    0
}
