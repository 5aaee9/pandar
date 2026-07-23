#pragma once

#include "shim_status_payload.hpp"

namespace pandar::network_plugin {

bool firmware_generation_current(Agent* agent, std::uint64_t expected_generation) {
    if (expected_generation == 0) return true;
    std::lock_guard<std::recursive_mutex> transition(agent->firmware_transition_mutex);
    return agent->firmware_generation == expected_generation;
}

BBL::OnMessageFn studio_message_callback(Agent* agent, MessageTunnel tunnel) {
    std::lock_guard<std::mutex> lock(agent->status_mutex);
    return tunnel == MessageTunnel::Cloud ? agent->on_message : agent->on_local_message;
}

bool deliver_prepared_message(
    Agent* agent,
    MessageTunnel tunnel,
    PluginStudioDeliveryResult delivery,
    StudioPayloadCopy payload,
    const std::string* body_override = nullptr,
    std::uint64_t firmware_generation = 0
) {
    if (delivery.status != 0 || delivery.ticket == 0) return false;
    std::lock_guard<std::recursive_timed_mutex> gate(agent->callback_mutex);
    if (!firmware_generation_current(agent, firmware_generation)) {
        pandar_plugin_studio_complete_delivery(
            agent->printer_refresh_session, delivery.ticket, false
        );
        return false;
    }
    if (pandar_plugin_studio_claim_delivery(
            agent->printer_refresh_session, delivery.ticket
        ) != 1) return false;
    auto callback = studio_message_callback(agent, tunnel);
    if (!callback) {
        pandar_plugin_studio_complete_delivery(
            agent->printer_refresh_session, delivery.ticket, false
        );
        return false;
    }
    callback(payload.dev_id, body_override ? *body_override : payload.body);
    return pandar_plugin_studio_complete_delivery(
        agent->printer_refresh_session, delivery.ticket, true
    ) == 1;
}

PluginStudioDeliveryResult prepare_studio_message(
    Agent* agent,
    MessageTunnel tunnel,
    const std::string& dev_id,
    std::uint64_t local_generation,
    bool initialize_cloud,
    std::uint64_t cache_generation,
    StudioPayloadCopy& payload
) {
    return pandar_plugin_studio_prepare_message(
        agent->printer_refresh_session,
        studio_tunnel(tunnel),
        reinterpret_cast<const uint8_t*>(dev_id.data()), dev_id.size(),
        local_generation,
        initialize_cloud,
        cache_generation,
        &payload,
        copy_studio_payload
    );
}

bool deliver_printer_message(
    Agent* agent,
    MessageTunnel tunnel,
    const std::string& dev_id,
    const std::string& body,
    std::uint64_t,
    std::uint64_t local_generation = 0,
    bool initialize_cloud = false,
    std::uint64_t cache_generation = 0,
    std::uint64_t firmware_generation = 0
) {
    StudioPayloadCopy payload;
    auto delivery = prepare_studio_message(
        agent, tunnel, studio_dev_id(dev_id), local_generation,
        initialize_cloud, cache_generation, payload
    );
    return deliver_prepared_message(
        agent, tunnel, delivery, std::move(payload), &body, firmware_generation
    );
}

bool deliver_printer_status_message(
    Agent* agent,
    MessageTunnel tunnel,
    const std::string& dev_id,
    std::uint64_t,
    std::uint64_t local_generation,
    std::uint64_t firmware_generation,
    std::uint64_t& cache_generation
) {
    StudioPayloadCopy payload;
    auto delivery = prepare_studio_message(
        agent, tunnel, studio_dev_id(dev_id), local_generation, false, 0, payload
    );
    cache_generation = delivery.cache_generation;
    return deliver_prepared_message(
        agent, tunnel, delivery, std::move(payload), nullptr, firmware_generation
    );
}

std::uint64_t current_local_printer_generation(
    const Agent* agent,
    const std::string& dev_id
) {
    if (!agent) return 0;
    return pandar_plugin_studio_local_generation(
        agent->printer_refresh_session,
        reinterpret_cast<const uint8_t*>(dev_id.data()), dev_id.size()
    );
}

bool emit_local_connect(Agent* agent, const std::string& dev_id) {
    if (!agent || dev_id.empty()) return false;
    StudioPayloadCopy payload;
    auto delivery = pandar_plugin_studio_connect_local(
        agent->printer_refresh_session,
        reinterpret_cast<const uint8_t*>(dev_id.data()), dev_id.size(),
        &payload,
        copy_studio_payload
    );
    if (delivery.status != 0 || delivery.ticket == 0) return false;
    std::lock_guard<std::recursive_timed_mutex> gate(agent->callback_mutex);
    if (pandar_plugin_studio_claim_delivery(
            agent->printer_refresh_session, delivery.ticket
        ) != 1) return false;
    BBL::OnLocalConnectedFn callback;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        callback = agent->on_local_connect;
    }
    if (!callback) {
        pandar_plugin_studio_complete_delivery(
            agent->printer_refresh_session, delivery.ticket, false
        );
        return false;
    }
    callback(0, payload.dev_id, payload.body);
    return pandar_plugin_studio_complete_delivery(
        agent->printer_refresh_session, delivery.ticket, true
    ) == 1;
}

} // namespace pandar::network_plugin
