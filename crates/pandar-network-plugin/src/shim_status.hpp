#pragma once

#include "shim_firmware_request.hpp"
#include "shim_status_delivery.hpp"
#include "shim_request_snapshot.hpp"

namespace pandar::network_plugin {

bool emit_cloud_printer_connected_signal(Agent* agent, const std::string& dev_id) {
    if (!agent || dev_id.empty()) return false;
    StudioPayloadCopy payload;
    auto delivery = pandar_plugin_studio_prepare_connected(
        agent->printer_refresh_session,
        reinterpret_cast<const uint8_t*>(dev_id.data()), dev_id.size(),
        studio_now_ms(),
        &payload,
        copy_studio_payload
    );
    if (delivery.status != 0 || delivery.ticket == 0) return false;
    std::lock_guard<std::recursive_timed_mutex> gate(agent->callback_mutex);
    if (pandar_plugin_studio_claim_delivery(
            agent->printer_refresh_session, delivery.ticket
        ) != 1) return false;
    BBL::OnPrinterConnectedFn callback;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        callback = agent->on_printer_connected;
    }
    if (!callback) {
        pandar_plugin_studio_complete_delivery(
            agent->printer_refresh_session, delivery.ticket, false
        );
        return false;
    }
    callback(payload.body);
    return pandar_plugin_studio_complete_delivery(
        agent->printer_refresh_session, delivery.ticket, true
    ) == 1;
}

bool emit_printer_status(
    Agent* agent,
    const std::string& dev_id,
    MessageTunnel tunnel,
    std::uint64_t local_generation = 0
) {
    if (!agent || dev_id.empty()) return false;
    std::uint64_t firmware_generation;
    {
        std::lock_guard<std::recursive_mutex> transition(agent->firmware_transition_mutex);
        firmware_generation = agent->firmware_generation;
    }
    std::uint64_t cache_generation = 0;
    const auto delivered = deliver_printer_status_message(
        agent, tunnel, dev_id, 0, local_generation,
        firmware_generation, cache_generation
    );
    trace_plugin_event(
        agent,
        std::string("push_status callbacks dev_id=") + dev_id +
            " tunnel=" + (tunnel == MessageTunnel::Cloud ? "cloud" : "local") +
            " callback=" + (delivered ? "1" : "0")
    );
    if (!delivered) return false;
    std::string firmware_body;
    bool firmware_ready = false;
    {
        std::lock_guard<std::recursive_mutex> transition(agent->firmware_transition_mutex);
        if (agent->firmware_generation != firmware_generation) return true;
        auto firmware = pandar_plugin_firmware_next_status_override(
            agent->firmware_session,
            reinterpret_cast<const uint8_t*>(dev_id.data()), dev_id.size()
        );
        firmware_body = body_from_result(firmware);
        firmware_ready = firmware.status == 0;
    }
    if (firmware_ready) {
        deliver_printer_message(
            agent, tunnel, dev_id, firmware_body, 0, local_generation,
            false, cache_generation, firmware_generation
        );
    }
    return true;
}

bool emit_printer_version(
    Agent* agent,
    const std::string& dev_id,
    const std::string& sequence_id,
    MessageTunnel tunnel,
    std::uint64_t local_generation = 0
) {
    if (!agent || dev_id.empty()) return false;
    const auto snapshot = printer_request_snapshot(agent, dev_id);
    auto admission = pandar_plugin_studio_request_admitted(
        snapshot.printer_authorized, snapshot.account_transition_pending
    );
    if (admission.status != 0) {
        body_from_result(admission);
        return false;
    }
    body_from_result(admission);
    auto version = firmware_version_from_snapshot(
        pandar_plugin_firmware_refresh_version,
        agent->firmware_session,
        dev_id,
        sequence_id,
        snapshot
    );
    auto version_body = body_from_result(version);
    if (version.status != 0) return false;
    const auto delivered = deliver_printer_message(
        agent, tunnel, dev_id, version_body, snapshot.account_epoch,
        local_generation, tunnel == MessageTunnel::Cloud,
        snapshot.cache_generation, snapshot.firmware_generation
    );
    trace_plugin_event(
        agent,
        std::string("get_version_response dev_id=") + dev_id +
            " tunnel=" + (tunnel == MessageTunnel::Cloud ? "cloud" : "local") +
            " callback=" + (delivered ? "1" : "0")
    );
    return delivered;
}

bool emit_cloud_printer_connected_status(Agent* agent, const std::string& dev_id) {
    emit_cloud_printer_connected_signal(agent, dev_id);
    return emit_printer_status(agent, dev_id, MessageTunnel::Cloud);
}

bool handle_studio_status(
    Agent* agent,
    int32_t kind,
    const std::string& dev_id,
    const std::string& sequence_id,
    MessageTunnel tunnel,
    std::uint64_t local_generation = 0
) {
    if (pandar_plugin_studio_status_target_available(
            agent->printer_refresh_session,
            studio_tunnel(tunnel),
            reinterpret_cast<const uint8_t*>(dev_id.data()), dev_id.size(),
            local_generation
        ) == 0) return false;
    if (kind == kStudioMessageGetVersion) {
        return emit_printer_version(
            agent, studio_dev_id(dev_id), sequence_id, tunnel, local_generation
        );
    }
    if (kind == kStudioMessagePushAll) {
        if (tunnel == MessageTunnel::Cloud) {
            emit_cloud_printer_connected_signal(agent, dev_id);
        }
        return emit_printer_status(agent, dev_id, tunnel, local_generation);
    }
    return false;
}

} // namespace pandar::network_plugin

#include "shim_status_heartbeat.hpp"
#include "shim_profile.hpp"
