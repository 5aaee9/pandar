//! Studio callback dispatch policy.
//!
//! The C++ shim owns the Studio-provided `std::function` callbacks and the
//! gate that synchronizes their lifetimes with Studio threads; it exposes
//! those as the flat [`ShimCallbackBridge`] vtable. This module owns the
//! Pandar-side delivery policy: claim-before-invoke, callback selection per
//! delivery kind, and the delivered/undelivered completion result.

use std::ffi::c_void;
use std::slice;

use super::super::ffi::session as connection_session;
use super::super::types::PluginConnectionResult;

const STUDIO_CALLBACK_SUCCESS: i32 = 0;
const STUDIO_CALLBACK_CONNECT_FAILED: i32 = -2;
const STUDIO_CALLBACK_AUTH_REJECTED: i32 = 5;

const STUDIO_WORK_CLOUD_MESSAGE: i32 = 1;
const STUDIO_WORK_LOCAL_MESSAGE: i32 = 2;
const STUDIO_WORK_LOCAL_CONNECTED: i32 = 3;

#[repr(C)]
pub struct ShimCallbackBridge {
    pub gate_lock: extern "C" fn(*mut c_void),
    pub gate_unlock: extern "C" fn(*mut c_void),
    pub status_thread_stopped: extern "C" fn(*mut c_void) -> i32,
    pub invoke_server_connected: extern "C" fn(*mut c_void, i32, i32) -> i32,
    pub invoke_message: extern "C" fn(*mut c_void, i32, *const u8, usize, *const u8, usize) -> i32,
    pub invoke_local_connected: extern "C" fn(*mut c_void, i32, *const u8, usize) -> i32,
}

struct CallbackGate<'a> {
    bridge: &'a ShimCallbackBridge,
    agent: *mut c_void,
}

impl<'a> CallbackGate<'a> {
    fn lock(bridge: &'a ShimCallbackBridge, agent: *mut c_void) -> Self {
        (bridge.gate_lock)(agent);
        Self { bridge, agent }
    }
}

impl Drop for CallbackGate<'_> {
    fn drop(&mut self) {
        (self.bridge.gate_unlock)(self.agent);
    }
}

fn bridge<'a>(bridge: *const ShimCallbackBridge) -> Option<&'a ShimCallbackBridge> {
    // SAFETY: the shim passes a pointer to its static bridge instance, which
    // outlives every dispatch call.
    unsafe { bridge.as_ref() }
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_shim_dispatch_connection_transition(
    bridge_ptr: *const ShimCallbackBridge,
    agent: *mut c_void,
    session_ptr: *mut c_void,
    result: PluginConnectionResult,
) {
    let (Some(bridge), Some(session)) = (bridge(bridge_ptr), connection_session(session_ptr))
    else {
        return;
    };
    if result.changed != 0 {
        let _gate = CallbackGate::lock(bridge, agent);
        if (bridge.status_thread_stopped)(agent) == 0
            && session.claim_delivery(result.transition_ticket)
        {
            let event = if result.connected != 0 {
                STUDIO_CALLBACK_SUCCESS
            } else {
                STUDIO_CALLBACK_CONNECT_FAILED
            };
            (bridge.invoke_server_connected)(agent, event, 0);
        }
    }
    if result.auth_changed != 0 {
        let _gate = CallbackGate::lock(bridge, agent);
        if (bridge.status_thread_stopped)(agent) != 0 {
            return;
        }
        if !session.claim_delivery(result.auth_ticket) {
            return;
        }
        if result.auth_rejected != 0 {
            (bridge.invoke_server_connected)(agent, STUDIO_CALLBACK_AUTH_REJECTED, 0);
        }
    }
}

/// Dispatches offline tickets and queued Studio work through the shim callback bridge.
///
/// # Safety
///
/// Non-null bridge and session pointers must remain valid for this call, and `agent` must
/// satisfy the callback bridge's lifetime contract. `offline_tickets` must be null or point
/// to `offline_len` initialized `u64` values that remain borrowed for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pandar_plugin_shim_dispatch_offline_deliveries(
    bridge_ptr: *const ShimCallbackBridge,
    agent: *mut c_void,
    session_ptr: *mut c_void,
    offline_tickets: *const u64,
    offline_len: usize,
) {
    let (Some(bridge), Some(session)) = (bridge(bridge_ptr), connection_session(session_ptr))
    else {
        return;
    };
    if !offline_tickets.is_null() {
        // SAFETY: the shim passes a pointer to offline_len consecutive tickets
        // that stay borrowed for the duration of this call.
        for &ticket in unsafe { slice::from_raw_parts(offline_tickets, offline_len) } {
            session.claim_delivery(ticket);
        }
    }
    for work in session.studio_take_work() {
        let _gate = CallbackGate::lock(bridge, agent);
        if !session.studio_claim_delivery(work.ticket) {
            continue;
        }
        let delivered = match work.kind {
            STUDIO_WORK_CLOUD_MESSAGE | STUDIO_WORK_LOCAL_MESSAGE => (bridge.invoke_message)(
                agent,
                work.kind,
                work.dev_id.as_ptr(),
                work.dev_id.len(),
                work.body.as_ptr(),
                work.body.len(),
            ),
            STUDIO_WORK_LOCAL_CONNECTED => (bridge.invoke_local_connected)(
                agent,
                work.state,
                work.dev_id.as_ptr(),
                work.dev_id.len(),
            ),
            _ => 0,
        };
        session.studio_complete_delivery(work.ticket, delivered != 0);
    }
}
