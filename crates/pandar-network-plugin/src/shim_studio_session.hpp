#pragma once

#include "shim_types.hpp"

namespace pandar::network_plugin {

extern "C" {

struct PluginStudioDeliveryResult {
    int32_t status;
    uint64_t ticket;
    uint64_t local_generation;
    uint64_t account_epoch;
    uint64_t cache_generation;
};

struct PluginStudioHeartbeatPlan {
    uint32_t wait_ms;
    int32_t refresh;
};

struct PluginStudioRequestState {
    int32_t status;
    int32_t authorized;
    int32_t account_transition_pending;
    uint64_t account_epoch;
    uint64_t cache_generation;
};

struct PluginStudioMessageResult {
    int32_t kind;
    int32_t outcome;
    int32_t abi_status;
    uint8_t* body_ptr;
    std::size_t body_len;
    std::size_t body_cap;
};

using StudioPayloadVisitor = void (*)(
    void*, const uint8_t*, std::size_t, const uint8_t*, std::size_t,
    const uint8_t*, std::size_t, const uint8_t*, std::size_t
);
using StudioHeartbeatVisitor = void (*)(
    void*, int32_t, const uint8_t*, std::size_t, uint64_t
);
using StudioWorkVisitor = void (*)(
    void*, int32_t, int32_t, uint64_t, uint64_t,
    const uint8_t*, std::size_t, const uint8_t*, std::size_t
);
using StudioRequestVisitor = void (*)(
    void*, const uint8_t*, std::size_t, const uint8_t*, std::size_t,
    const uint8_t*, std::size_t
);

int32_t pandar_plugin_studio_set_listener(void*, int32_t, bool);
PluginHttpResult pandar_plugin_studio_selected(void*);
int32_t pandar_plugin_studio_set_selected(void*, const uint8_t*, std::size_t);
int32_t pandar_plugin_studio_add_subscription(void*, const uint8_t*, std::size_t);
int32_t pandar_plugin_studio_del_subscription(void*, const uint8_t*, std::size_t);
PluginStudioHeartbeatPlan pandar_plugin_studio_heartbeat_plan(
    void*, void*, StudioHeartbeatVisitor
);
PluginStudioDeliveryResult pandar_plugin_studio_prepare_connected(
    void*, const uint8_t*, std::size_t, uint64_t, void*, StudioPayloadVisitor
);
PluginStudioDeliveryResult pandar_plugin_studio_prepare_message(
    void*, int32_t, const uint8_t*, std::size_t, uint64_t, bool, uint64_t,
    void*, StudioPayloadVisitor
);
int32_t pandar_plugin_studio_status_target_available(
    void*, int32_t, const uint8_t*, std::size_t, uint64_t
);
PluginStudioDeliveryResult pandar_plugin_studio_connect_local(
    void*, const uint8_t*, std::size_t, void*, StudioPayloadVisitor
);
int32_t pandar_plugin_studio_disconnect_local(void*);
uint64_t pandar_plugin_studio_local_generation(void*, const uint8_t*, std::size_t);
int32_t pandar_plugin_studio_complete_delivery(void*, uint64_t, bool);
int32_t pandar_plugin_studio_claim_delivery(void*, uint64_t);
int32_t pandar_plugin_studio_take_work(void*, void*, StudioWorkVisitor);
int32_t pandar_plugin_studio_begin_account_transition(void*);
int32_t pandar_plugin_studio_finish_account_transition(void*, uint64_t);
PluginStudioRequestState pandar_plugin_studio_request_snapshot(
    void*, const uint8_t*, std::size_t, void*, StudioRequestVisitor
);
int32_t pandar_plugin_connection_studio_snapshot_current(void*, uint64_t, uint64_t);
int32_t pandar_plugin_studio_account_request_admitted(void*);
int32_t pandar_plugin_studio_account_request_current(void*, uint64_t);
PluginStudioMessageResult pandar_plugin_dispatch_studio_message(const uint8_t*, std::size_t);

} // extern "C"

constexpr int32_t kStudioCloudTunnel = 0;
constexpr int32_t kStudioLocalTunnel = 1;
constexpr int32_t kStudioCloudListener = 1;
constexpr int32_t kStudioLocalListener = 2;
constexpr int32_t kStudioPrinterConnectedListener = 3;
constexpr int32_t kStudioLocalConnectedListener = 4;
constexpr int32_t kStudioMessageFirmware = 1;
constexpr int32_t kStudioMessageGetVersion = 2;
constexpr int32_t kStudioMessagePushAll = 3;
constexpr int32_t kStudioMessageOperation = 4;
constexpr int32_t kStudioMessageH2cAutoNozzleMapping = 5;

struct StudioPayloadCopy {
    std::string dev_id;
    std::string body;
    std::string printer_id;
    std::string model;
};

inline void copy_studio_payload(
    void* context,
    const uint8_t* dev_id, std::size_t dev_id_len,
    const uint8_t* body, std::size_t body_len,
    const uint8_t* printer_id, std::size_t printer_id_len,
    const uint8_t* model, std::size_t model_len
) {
    auto& copy = *static_cast<StudioPayloadCopy*>(context);
    copy.dev_id.assign(reinterpret_cast<const char*>(dev_id), dev_id_len);
    copy.body.assign(reinterpret_cast<const char*>(body), body_len);
    copy.printer_id.assign(reinterpret_cast<const char*>(printer_id), printer_id_len);
    copy.model.assign(reinterpret_cast<const char*>(model), model_len);
}

inline std::string body_from_studio_message(PluginStudioMessageResult result) {
    std::string body;
    if (result.body_ptr && result.body_len) {
        body.assign(reinterpret_cast<const char*>(result.body_ptr), result.body_len);
    }
    pandar_plugin_free_with_capacity(result.body_ptr, result.body_len, result.body_cap);
    return body;
}

inline int32_t studio_tunnel(MessageTunnel tunnel) {
    return tunnel == MessageTunnel::Cloud ? kStudioCloudTunnel : kStudioLocalTunnel;
}

inline uint64_t studio_now_ms() {
    return static_cast<uint64_t>(std::chrono::duration_cast<std::chrono::milliseconds>(
        std::chrono::steady_clock::now().time_since_epoch()
    ).count());
}

inline PluginStudioRequestState studio_session_state(const Agent* agent) {
    static const uint8_t empty[1] = {0};
    if (!agent) return {-1, 0, 0, 0, 0};
    return pandar_plugin_studio_request_snapshot(
        agent->printer_refresh_session, empty, 0, nullptr, nullptr
    );
}

} // namespace pandar::network_plugin
