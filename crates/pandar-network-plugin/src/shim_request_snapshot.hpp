#pragma once

#include "shim_state.hpp"
#include "shim_print_types.hpp"
#include "shim_request_types.hpp"

namespace pandar::network_plugin {

PrinterRequestSnapshot printer_request_snapshot(
    const Agent* agent,
    const std::string& dev_id
) {
    PrinterRequestSnapshot snapshot;
    if (!agent) return snapshot;
    const auto normalized_dev_id = studio_dev_id(dev_id);
    std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
    auto copy = [](void* context,
                   const uint8_t* hub, std::size_t hub_len,
                   const uint8_t* token, std::size_t token_len,
                   const uint8_t* printer, std::size_t printer_len) {
        auto& target = *static_cast<PrinterRequestSnapshot*>(context);
        target.hub_url.assign(reinterpret_cast<const char*>(hub), hub_len);
        target.token.assign(reinterpret_cast<const char*>(token), token_len);
        target.printer_id.assign(reinterpret_cast<const char*>(printer), printer_len);
    };
    const auto state = pandar_plugin_studio_request_snapshot(
        agent->connection_session(),
        reinterpret_cast<const uint8_t*>(normalized_dev_id.data()), normalized_dev_id.size(),
        &snapshot, copy
    );
    snapshot.printer_authorized = state.authorized != 0;
    snapshot.account_transition_pending = state.account_transition_pending != 0;
    snapshot.account_epoch = state.account_epoch;
    snapshot.account_config_epoch = agent->account_config_epoch.load(std::memory_order_acquire);
    snapshot.session_kind = agent->account_session_kind;
    snapshot.cache_generation = state.cache_generation;
    snapshot.firmware_generation =
        pandar_plugin_firmware_session_generation(agent->firmware_session());
    return snapshot;
}

inline PluginStudioSnapshot plugin_studio_snapshot(const PrinterRequestSnapshot& snapshot) {
    return {
        plugin_bytes(snapshot.hub_url),
        plugin_bytes(snapshot.token),
        plugin_bytes(snapshot.printer_id),
        static_cast<std::uint8_t>(snapshot.printer_authorized),
        static_cast<std::uint8_t>(snapshot.account_transition_pending),
        snapshot.account_epoch,
        snapshot.cache_generation,
        snapshot.firmware_generation,
    };
}

bool printer_request_snapshot_current(
    const Agent* agent,
    const PrinterRequestSnapshot& snapshot
) {
    if (!agent) return false;
    if (pandar_plugin_connection_studio_snapshot_current(
            agent->connection_session(),
            snapshot.account_epoch,
            snapshot.cache_generation
        ) == 0) return false;
    return pandar_plugin_firmware_session_generation_current(
        agent->firmware_session(), snapshot.firmware_generation
    ) != 0;
}

} // namespace pandar::network_plugin
