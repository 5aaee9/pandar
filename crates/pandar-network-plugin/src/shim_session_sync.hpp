#pragma once

namespace pandar::network_plugin {

void invalidate_firmware_account_session(Agent* agent) {
    if (!agent || !agent->firmware_session) return;
    agent->firmware_transition_pending = true;
    std::lock_guard<std::recursive_mutex> transition(agent->firmware_transition_mutex);
    pandar_plugin_firmware_cancel_generation(
        agent->firmware_session,
        agent->firmware_generation
    );
    ++agent->firmware_generation;
    pandar_plugin_firmware_session_update(
        agent->firmware_session,
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size(),
        agent->firmware_generation
    );
    agent->firmware_hub_url = agent->hub_url;
    agent->firmware_token = agent->token;
    agent->firmware_transition_pending = false;
}

void sync_firmware_session(Agent* agent) {
    if (!agent || !agent->firmware_session) return;
    agent->firmware_transition_pending = true;
    std::lock_guard<std::recursive_mutex> transition(agent->firmware_transition_mutex);
    if (agent->firmware_hub_url == agent->hub_url && agent->firmware_token == agent->token) {
        agent->firmware_transition_pending = false;
        return;
    }
    const auto previous_generation = agent->firmware_generation;
    pandar_plugin_firmware_cancel_generation(agent->firmware_session, previous_generation);
    ++agent->firmware_generation;
    pandar_plugin_firmware_session_update(
        agent->firmware_session,
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size(),
        agent->firmware_generation
    );
    agent->firmware_hub_url = agent->hub_url;
    agent->firmware_token = agent->token;
    agent->firmware_transition_pending = false;
}

void sync_printer_refresh_session(Agent* agent) {
    if (!agent || !agent->printer_refresh_session) return;
    std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
    pandar_plugin_printer_refresh_session_update(
        agent->printer_refresh_session,
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size()
    );
    pandar_plugin_printer_refresh_session_set_tenant(
        agent->printer_refresh_session,
        reinterpret_cast<const uint8_t*>(agent->tenant_id.data()),
        agent->tenant_id.size()
    );
    sync_firmware_session(agent);
}

} // namespace pandar::network_plugin
