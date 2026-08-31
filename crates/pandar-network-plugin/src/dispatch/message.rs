use super::*;

struct RequestFacts {
    snapshot: StudioRequestSnapshot,
    account_epoch: u64,
    cache_generation: u64,
}

/// Snapshot plus admission gate shared by every printer-scoped route.
fn admit_request(session: &ConnectionSession, dev_id: &str) -> Result<RequestFacts, i32> {
    let (state, snapshot) = session.studio_request_snapshot(dev_id.to_owned());
    let admission =
        studio_request_admitted(state.authorized != 0, state.account_transition_pending != 0);
    let (status, _, _) = take_http(admission);
    if status != 0 {
        return Err(status);
    }
    Ok(RequestFacts {
        snapshot,
        account_epoch: state.account_epoch,
        cache_generation: state.cache_generation,
    })
}

fn snapshot_currency(
    bridge: &PluginDispatchBridge,
    agent: *mut c_void,
    session: &ConnectionSession,
    facts: &RequestFacts,
    firmware_generation: u64,
) -> bool {
    session.studio_request_snapshot_current(facts.account_epoch, facts.cache_generation)
        && (bridge.firmware_generation_current)(agent, firmware_generation) != 0
}

/// Shared per-message dispatch state: the callback bridge, the target agent,
/// and the sessions and fences one routed message operates on.
pub(super) struct DispatchContext<'a> {
    pub(super) bridge: &'a PluginDispatchBridge,
    pub(super) agent: *mut c_void,
    pub(super) session: &'a ConnectionSession,
    pub(super) firmware_session: *mut c_void,
    pub(super) tunnel: i32,
    pub(super) local_generation: u64,
    pub(super) firmware_generation: u64,
}

/// Prepares and invokes one Studio message delivery under the callback gate.
///
/// Fences on the firmware generation when one is given, claims before
/// invoking, and completes with the callback's delivered verdict. Returns the
/// delivered verdict and the delivery's cache generation.
pub(super) fn deliver_message(
    context: &DispatchContext<'_>,
    dev_id: &str,
    initialize_cloud: bool,
    expected_cache_generation: u64,
    body_override: Option<&str>,
) -> (bool, u64) {
    let (delivery, payload) = context.session.studio_prepare_message(
        context.tunnel,
        dev_id.to_owned(),
        context.local_generation,
        initialize_cloud,
        expected_cache_generation,
    );
    if delivery.status != 0 || delivery.ticket == 0 {
        return (false, delivery.cache_generation);
    }
    let cache_generation = delivery.cache_generation;
    let _gate = CallbackGate::lock(context.bridge, context.agent);
    if (context.bridge.firmware_generation_current)(context.agent, context.firmware_generation) == 0
    {
        context
            .session
            .studio_complete_delivery(delivery.ticket, false);
        return (false, cache_generation);
    }
    if !context.session.studio_claim_delivery(delivery.ticket) {
        return (false, cache_generation);
    }
    let Some(payload) = payload else {
        return (false, cache_generation);
    };
    let body = body_override.unwrap_or(&payload.body);
    let delivered = (context.bridge.base.invoke_message)(
        context.agent,
        tunnel_work_kind(context.tunnel),
        payload.dev_id.as_ptr(),
        payload.dev_id.len(),
        body.as_ptr(),
        body.len(),
    ) != 0;
    let completed = context
        .session
        .studio_complete_delivery(delivery.ticket, delivered);
    (completed, cache_generation)
}

/// Prepares and delivers the printer-connected signal.
fn emit_cloud_printer_connected(
    bridge: &PluginDispatchBridge,
    agent: *mut c_void,
    session: &ConnectionSession,
    dev_id: &str,
) -> bool {
    let (delivery, payload) =
        session.studio_prepare_connected(dev_id.to_owned(), (bridge.now_ms)(agent));
    if delivery.status != 0 || delivery.ticket == 0 {
        return false;
    }
    let _gate = CallbackGate::lock(bridge, agent);
    if !session.studio_claim_delivery(delivery.ticket) {
        return false;
    }
    let Some(payload) = payload else {
        return false;
    };
    let delivered =
        (bridge.base.invoke_printer_connected)(agent, payload.body.as_ptr(), payload.body.len())
            != 0;
    session.studio_complete_delivery(delivery.ticket, delivered)
}

/// Pushes the cached printer status plus any pending firmware status
/// override.
fn emit_printer_status(context: &DispatchContext<'_>, dev_id: &str) -> bool {
    let (delivered, cache_generation) = deliver_message(context, dev_id, false, 0, None);
    trace(
        context.bridge,
        context.agent,
        &format!(
            "push_status callbacks dev_id={dev_id} tunnel={} callback={}",
            tunnel_label(context.tunnel),
            i32::from(delivered)
        ),
    );
    if !delivered {
        return false;
    }
    if (context.bridge.firmware_generation_current)(context.agent, context.firmware_generation) == 0
    {
        return true;
    }
    let Some(firmware_session) = (unsafe { firmware_session_ref(context.firmware_session) }) else {
        return true;
    };
    let Some(override_body) = firmware_session.next_status_override(dev_id) else {
        return true;
    };
    deliver_message(
        context,
        dev_id,
        false,
        cache_generation,
        Some(&override_body),
    );
    true
}

/// Answers `info.get_version` from the firmware session.
fn emit_printer_version(
    context: &DispatchContext<'_>,
    facts: &RequestFacts,
    dev_id: &str,
    sequence_id: &str,
) -> bool {
    let Some(firmware_session) = (unsafe { firmware_session_ref(context.firmware_session) }) else {
        return false;
    };
    let version_body = firmware_session.refresh_version_json(
        &facts.snapshot.printer_id,
        sequence_id,
        context.firmware_generation,
    );
    let (delivered, _) = deliver_message(
        context,
        dev_id,
        context.tunnel == CLOUD_TUNNEL,
        facts.cache_generation,
        Some(&version_body),
    );
    trace(
        context.bridge,
        context.agent,
        &format!(
            "get_version_response dev_id={dev_id} tunnel={} callback={}",
            tunnel_label(context.tunnel),
            i32::from(delivered)
        ),
    );
    delivered
}

/// Routes one firmware-classified message into the firmware session and
/// returns the handoff for its callback.
fn dispatch_firmware_message(context: &DispatchContext<'_>, dev_id: &str, message: &str) -> i32 {
    let facts = match admit_request(context.session, dev_id) {
        Ok(facts) => facts,
        Err(status) => return status,
    };
    let Some(firmware_session) = (unsafe { firmware_session_ref(context.firmware_session) }) else {
        return ABI_INVALID_RESULT;
    };
    let firmware_tunnel = if context.tunnel == CLOUD_TUNNEL {
        FirmwareTunnel::Cloud
    } else {
        FirmwareTunnel::Local
    };
    let response = firmware_session.send(
        dev_id,
        &facts.snapshot.printer_id,
        message,
        firmware_tunnel,
        context.firmware_generation,
    );
    if matches!(response.outcome, FirmwareSendOutcome::PrePublishFailure) {
        return ABI_INVALID_RESULT;
    }
    if let Some(token) = response.callback_token {
        firmware_session.return_handoff_at(
            token,
            (context.bridge.steady_tick_ns)(context.agent),
            context.local_generation,
            facts.cache_generation,
            std::time::Instant::now(),
        );
    }
    ABI_SUCCESS
}

/// Normalizes one upstream printer-operation submission into the ABI status
/// the shim used to compute inline.
fn printer_operation_status(
    context: &DispatchContext<'_>,
    facts: &RequestFacts,
    upstream: PluginHttpResult,
) -> (i32, String) {
    let (upstream_status, upstream_http_code, body) = take_http(upstream);
    let normalized = studio_printer_operation_result(
        upstream_status,
        upstream_http_code,
        body,
        snapshot_currency(
            context.bridge,
            context.agent,
            context.session,
            facts,
            context.firmware_generation,
        ),
    );
    let (status, _, body) = take_http(normalized);
    (status, body)
}

/// Routes one classified Studio message to its session, HTTP, or firmware
/// path and reports the Studio ABI status. `pandar_plugin_dispatch_message`
/// is the whole former C++ `dispatch_studio_message` routing policy.
///
/// # Safety
///
/// The bridge must point to the shim's static bridge, `agent` must satisfy
/// its lifetime contract, and the request's pointers must be valid for the
/// call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pandar_plugin_dispatch_message(
    bridge_ptr: *const PluginDispatchBridge,
    agent: *mut c_void,
    request: PluginDispatchMessageRequest,
) -> i32 {
    let (Some(bridge), Some(session)) = (unsafe { bridge(bridge_ptr) }, unsafe {
        connection_session(request.session)
    }) else {
        return ABI_INVALID_RESULT;
    };
    let Some(dev_id) = (unsafe { read_utf8(request.dev_id_ptr, request.dev_id_len) }) else {
        return ABI_CONNECT_FAILED;
    };
    let Some(message) = (unsafe { read_utf8(request.message_ptr, request.message_len) }) else {
        return ABI_INVALID_RESULT;
    };
    let context = DispatchContext {
        bridge,
        agent,
        session,
        firmware_session: request.firmware_session,
        tunnel: request.tunnel,
        local_generation: request.local_generation,
        firmware_generation: request.firmware_generation,
    };

    match classify_studio_message(&message) {
        StudioMessageRoute::Invalid { .. } => ABI_INVALID_RESULT,
        StudioMessageRoute::Firmware => {
            dispatch_firmware_message(&context, &normalize_studio_dev_id(dev_id.clone()), &message)
        }
        StudioMessageRoute::GetVersion { sequence_id } => {
            let dev_id = normalize_studio_dev_id(dev_id);
            if !session.studio_status_target_available(
                context.tunnel,
                dev_id.clone(),
                context.local_generation,
            ) {
                return ABI_CONNECT_FAILED;
            }
            // A rejected request reports the same undelivered outcome as a
            // missing callback target; the admission reason is not surfaced
            // to Studio on this route.
            let facts = match admit_request(session, &dev_id) {
                Ok(facts) => facts,
                Err(_) => return ABI_CONNECT_FAILED,
            };
            if emit_printer_version(&context, &facts, &dev_id, &sequence_id) {
                ABI_SUCCESS
            } else {
                ABI_CONNECT_FAILED
            }
        }
        StudioMessageRoute::PushAll { .. } => {
            if !session.studio_status_target_available(
                context.tunnel,
                dev_id.clone(),
                context.local_generation,
            ) {
                return ABI_CONNECT_FAILED;
            }
            if context.tunnel == CLOUD_TUNNEL {
                emit_cloud_printer_connected(context.bridge, agent, session, &dev_id);
            }
            if emit_printer_status(&context, &dev_id) {
                ABI_SUCCESS
            } else {
                ABI_CONNECT_FAILED
            }
        }
        StudioMessageRoute::H2cAutoNozzleMapping { request_json } => {
            (bridge.refresh_local_webserver)(agent);
            let facts = match admit_request(session, &dev_id) {
                Ok(facts) => facts,
                Err(status) => return status,
            };
            let upstream = submit_h2c_auto_nozzle_mapping_upstream(
                &facts.snapshot.hub_url,
                &facts.snapshot.token,
                &facts.snapshot.printer_id,
                &request_json,
            );
            let (status, body) = printer_operation_status(&context, &facts, upstream);
            if status != 0 {
                return status;
            }
            let (delivered, _) = deliver_message(
                &context,
                &dev_id,
                false,
                facts.cache_generation,
                Some(&body),
            );
            if delivered {
                ABI_SUCCESS
            } else {
                ABI_INVALID_RESULT
            }
        }
        StudioMessageRoute::Operation { operation_json } => {
            (bridge.refresh_local_webserver)(agent);
            let facts = match admit_request(session, &dev_id) {
                Ok(facts) => facts,
                Err(status) => return status,
            };
            let upstream = submit_printer_operation_upstream(
                &facts.snapshot.hub_url,
                &facts.snapshot.token,
                &facts.snapshot.printer_id,
                &operation_json,
            );
            let (status, _) = printer_operation_status(&context, &facts, upstream);
            status
        }
    }
}
