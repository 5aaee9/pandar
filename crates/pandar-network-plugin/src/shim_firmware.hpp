#pragma once

#include "shim_firmware_request.hpp"
#include "shim_request_snapshot.hpp"
#include "shim_session_sync.hpp"
#include "shim_dispatch.hpp"

namespace pandar::network_plugin {

std::string string_from_firmware_allocation(uint8_t* ptr, std::size_t len, std::size_t cap) {
    std::string value;
    if (ptr && len > 0) value.assign(reinterpret_cast<char*>(ptr), len);
    pandar_plugin_free_with_capacity(ptr, len, cap);
    return value;
}

FirmwareObservationTicket begin_firmware_observation(Agent* agent) {
    if (!agent) return {};
    std::lock_guard<std::recursive_mutex> transition(agent->firmware_transition_mutex);
    return {
        agent->firmware_generation,
        ++agent->firmware_observation_sequence,
    };
}

void start_firmware_dispatcher(Agent* agent) {
    if (!agent || agent->firmware_thread.joinable()) return;
    agent->firmware_thread_stop = false;
    agent->firmware_thread = std::thread([agent] {
        while (!agent->firmware_thread_stop.load()) {
            if (agent->firmware_transition_pending.load()) {
                std::this_thread::yield();
                continue;
            }
            const auto taken = dispatch_firmware_callback(agent);
            if (taken != 0) {
                std::this_thread::yield();
            } else {
                std::this_thread::sleep_for(std::chrono::milliseconds(1));
            }
        }
    });
}

void stop_firmware_dispatcher(Agent* agent) {
    if (!agent) return;
    agent->firmware_thread_stop = true;
    pandar_plugin_firmware_stop(agent->firmware_session);
    if (agent->firmware_thread.joinable()) agent->firmware_thread.join();
}

PluginHttpResult rust_start_local_webserver(const Agent* agent) {
    return pandar_plugin_start_local_webserver(
        reinterpret_cast<const uint8_t*>(agent->frontend_url.data()),
        agent->frontend_url.size(),
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        agent->frontend_configured,
        agent->hub_configured
    );
}

void refresh_local_webserver_config(Agent* agent) {
    auto lifecycle = pandar_plugin_account_refresh_runtime(agent, with_current_account);
    body_from_result(lifecycle.http);
}

} // namespace pandar::network_plugin

#include "shim_status_heartbeat.hpp"
#include "shim_profile.hpp"
#include "shim_no_auth.hpp"
