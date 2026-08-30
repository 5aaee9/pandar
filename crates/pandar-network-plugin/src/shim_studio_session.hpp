#pragma once

#include "shim_types.hpp"

namespace pandar::network_plugin {

extern "C" {

struct PluginStudioRequestState {
    int32_t status;
    int32_t authorized;
    int32_t account_transition_pending;
    uint64_t account_epoch;
    uint64_t cache_generation;
};

int32_t pandar_plugin_studio_set_listener(void*, int32_t, bool);
PluginHttpResult pandar_plugin_studio_selected(void*);
int32_t pandar_plugin_studio_set_selected(void*, const uint8_t*, std::size_t);
int32_t pandar_plugin_studio_add_subscription(void*, const uint8_t*, std::size_t);
int32_t pandar_plugin_studio_del_subscription(void*, const uint8_t*, std::size_t);
int32_t pandar_plugin_studio_disconnect_local(void*);
uint64_t pandar_plugin_studio_local_generation(void*, const uint8_t*, std::size_t);
int32_t pandar_plugin_studio_begin_account_transition(void*);
int32_t pandar_plugin_studio_finish_account_transition(void*, uint64_t);
PluginStudioRequestState pandar_plugin_studio_request_snapshot(
    void*, const uint8_t*, std::size_t, void*, void (*)(
        void*, const uint8_t*, std::size_t, const uint8_t*, std::size_t,
        const uint8_t*, std::size_t
    )
);
int32_t pandar_plugin_connection_studio_snapshot_current(void*, uint64_t, uint64_t);

} // extern "C"

constexpr int32_t kStudioCloudListener = 1;
constexpr int32_t kStudioLocalListener = 2;
constexpr int32_t kStudioPrinterConnectedListener = 3;
constexpr int32_t kStudioLocalConnectedListener = 4;

inline uint64_t studio_now_ms() {
    return static_cast<uint64_t>(std::chrono::duration_cast<std::chrono::milliseconds>(
        std::chrono::steady_clock::now().time_since_epoch()
    ).count());
}

inline PluginStudioRequestState studio_session_state(const Agent* agent) {
    static const uint8_t empty[1] = {0};
    if (!agent) return {-1, 0, 0, 0, 0};
    return pandar_plugin_studio_request_snapshot(
        agent->connection_session(), empty, 0, nullptr, nullptr
    );
}

} // namespace pandar::network_plugin
