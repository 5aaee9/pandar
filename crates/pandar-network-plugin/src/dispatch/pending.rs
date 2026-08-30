use super::message::{DispatchContext, deliver_message};
use super::*;

/// Answers `bambu_network_connect_printer`: prepares the local connect
/// delivery and invokes the Studio local-connected callback.
///
/// # Safety
///
/// The bridge must point to the shim's static bridge and `session` must be a
/// live connection session for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pandar_plugin_dispatch_connect_local(
    bridge_ptr: *const PluginDispatchBridge,
    agent: *mut c_void,
    session_ptr: *mut c_void,
    dev_id_ptr: *const u8,
    dev_id_len: usize,
) -> i32 {
    let (Some(bridge), Some(session)) = (bridge(bridge_ptr), connection_session(session_ptr))
    else {
        return ABI_CONNECT_FAILED;
    };
    let Some(dev_id) = read_utf8(dev_id_ptr, dev_id_len).filter(|dev_id| !dev_id.is_empty()) else {
        return ABI_CONNECT_FAILED;
    };
    let (delivery, payload) = session.studio_connect_local(dev_id);
    if delivery.status != 0 || delivery.ticket == 0 {
        return ABI_CONNECT_FAILED;
    }
    let _gate = CallbackGate::lock(bridge, agent);
    if !session.studio_claim_delivery(delivery.ticket) {
        return ABI_CONNECT_FAILED;
    }
    let Some(payload) = payload else {
        return ABI_CONNECT_FAILED;
    };
    let delivered = (bridge.invoke_local_connected_with_body)(
        agent,
        0,
        payload.dev_id.as_ptr(),
        payload.dev_id.len(),
        payload.body.as_ptr(),
        payload.body.len(),
    ) != 0;
    if session.studio_complete_delivery(delivery.ticket, delivered) {
        ABI_SUCCESS
    } else {
        ABI_CONNECT_FAILED
    }
}

/// Takes one ready firmware callback, waits for its handoff window, and
/// delivers it through the Studio message callback. Returns `1` when a
/// callback was taken (the caller yields) and `0` when none was ready (the
/// caller sleeps).
///
/// # Safety
///
/// The bridge must point to the shim's static bridge, `agent` must satisfy
/// its lifetime contract, and both session pointers must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pandar_plugin_dispatch_firmware_callback(
    bridge_ptr: *const PluginDispatchBridge,
    agent: *mut c_void,
    session_ptr: *mut c_void,
    firmware_session_ptr: *mut c_void,
) -> i32 {
    let (Some(bridge), Some(session), Some(firmware_session)) = (
        bridge(bridge_ptr),
        connection_session(session_ptr),
        unsafe { firmware_session_ref(firmware_session_ptr) },
    ) else {
        return 0;
    };
    let Some(callback) =
        firmware_session.wait_ready_callback(Duration::from_millis(FIRMWARE_CALLBACK_WAIT_MS))
    else {
        return 0;
    };
    let tunnel = if matches!(callback.tunnel, FirmwareTunnel::Cloud) {
        CLOUD_TUNNEL
    } else {
        LOCAL_TUNNEL
    };
    let context = DispatchContext {
        bridge,
        agent,
        session,
        firmware_session: session_ptr,
        tunnel,
        local_generation: callback.local_generation,
        firmware_generation: callback.generation,
    };
    let deadline_ns = callback
        .origin_tick
        .saturating_add(FIRMWARE_CALLBACK_DEADLINE_NS);
    if (bridge.gate_try_lock_until)(agent, deadline_ns) == 0 {
        return 1;
    }
    if (bridge.steady_tick_ns)(agent) < deadline_ns {
        deliver_message(
            &context,
            &callback.dev_id,
            false,
            callback.cache_generation,
            Some(&callback.message),
        );
    }
    (bridge.base.gate_unlock)(agent);
    1
}

/// Runs one status-heartbeat iteration: sync the streamed firmware
/// projection, drain transitions, offline deliveries, queued Studio work and
/// the pending stream error, retry the no-auth session when the caller's
/// retry window elapsed, and report the plan for the next wait.
///
/// # Safety
///
/// The bridge must point to the shim's static bridge, `agent` must satisfy
/// its lifetime contract, and both session pointers must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pandar_plugin_dispatch_pending(
    bridge_ptr: *const PluginDispatchBridge,
    agent: *mut c_void,
    session_ptr: *mut c_void,
    firmware_session_ptr: *mut c_void,
    no_auth_retry_due: i32,
) -> PluginPendingOutcome {
    let Some(bridge) = bridge(bridge_ptr) else {
        return PluginPendingOutcome {
            wait_ms: NO_AUTH_RETRY_DELAY_MS,
            logged_out: 1,
        };
    };
    let Some(session) = connection_session(session_ptr) else {
        return PluginPendingOutcome {
            wait_ms: NO_AUTH_RETRY_DELAY_MS,
            logged_out: 1,
        };
    };
    (bridge.sync_firmware)(agent, firmware_session_ptr);
    dispatch_pending_deliveries(bridge, agent, session_ptr, session);
    let logged_out = session.is_logged_out();
    if no_auth_retry_due != 0 && logged_out {
        (bridge.retry_no_auth)(agent);
        dispatch_pending_deliveries(bridge, agent, session_ptr, session);
    }
    let (plan, _) = session.studio_heartbeat_plan();
    PluginPendingOutcome {
        wait_ms: plan.wait_ms,
        logged_out: i32::from(logged_out),
    }
}

fn dispatch_pending_deliveries(
    bridge: &PluginDispatchBridge,
    agent: *mut c_void,
    session_ptr: *mut c_void,
    session: &ConnectionSession,
) {
    if !session.studio_account_request_admitted() {
        return;
    }
    let transition = session.take_transition();
    let offline: Vec<u64> = session
        .take_offline()
        .into_iter()
        .map(|issued| issued.ticket)
        .collect();
    dispatch_transition_and_tickets(bridge, agent, session_ptr, session, transition, &offline);
}

pub(crate) fn dispatch_transition_and_tickets(
    bridge: &PluginDispatchBridge,
    agent: *mut c_void,
    session_ptr: *mut c_void,
    session: &ConnectionSession,
    transition: crate::connection::PluginConnectionResult,
    offline_tickets: &[u64],
) {
    if transition.changed != 0 || transition.auth_changed != 0 {
        pandar_plugin_shim_dispatch_connection_transition(
            &bridge.base,
            agent,
            session_ptr,
            transition,
        );
    }
    // SAFETY: the tickets are borrowed for this call.
    unsafe {
        pandar_plugin_shim_dispatch_offline_deliveries(
            &bridge.base,
            agent,
            session_ptr,
            offline_tickets.as_ptr(),
            offline_tickets.len(),
        );
    }
    let error = session.take_stream_error();
    let (status, http_code, body) = take_http(error);
    if status != 0 {
        (bridge.invoke_http_error)(agent, http_code, body.as_ptr(), body.len());
    }
}

/// Drains the transitions, offline tickets, queued Studio work, and pending
/// stream error collected by one `pandar_plugin_printer_refresh_with_session`
/// transaction.
///
/// # Safety
///
/// The bridge must point to the shim's static bridge, `agent` must satisfy
/// its lifetime contract, `session` must be live, and `offline_tickets` must
/// be null or point to `offline_len` initialized values borrowed for the
/// call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pandar_plugin_dispatch_refresh_drain(
    bridge_ptr: *const PluginDispatchBridge,
    agent: *mut c_void,
    session_ptr: *mut c_void,
    transition: crate::connection::PluginConnectionResult,
    offline_tickets: *const u64,
    offline_len: usize,
) {
    let (Some(bridge), Some(session)) = (bridge(bridge_ptr), connection_session(session_ptr))
    else {
        return;
    };
    // SAFETY: the caller borrows the ticket slice for this call.
    let tickets = if offline_tickets.is_null() {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(offline_tickets, offline_len) }
    };
    dispatch_transition_and_tickets(bridge, agent, session_ptr, session, transition, tickets);
}
