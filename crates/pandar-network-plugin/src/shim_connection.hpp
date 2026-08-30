#pragma once

#include "shim_studio_session.hpp"

namespace pandar::network_plugin {

std::string body_from_result(PluginHttpResult);

extern "C" {

struct PluginConnectionResult {
    int32_t status;
    uint32_t http_code;
    int32_t connected;
    int32_t changed;
    int32_t auth_rejected;
    int32_t auth_changed;
    uint64_t transition_ticket;
    uint64_t auth_ticket;
};

struct PandarShimBridge {
    void (*gate_lock)(void*);
    void (*gate_unlock)(void*);
    int32_t (*status_thread_stopped)(void*);
    int32_t (*invoke_server_connected)(void*, int32_t, int32_t);
    int32_t (*invoke_message)(
        void*, int32_t, const uint8_t*, std::size_t, const uint8_t*, std::size_t
    );
    int32_t (*invoke_local_connected)(void*, int32_t, const uint8_t*, std::size_t);
    int32_t (*invoke_printer_connected)(void*, const uint8_t*, std::size_t);
    int32_t (*invoke_firmware_status)(void*, int32_t, const uint8_t*, std::size_t);
};

void pandar_plugin_shim_dispatch_connection_transition(
    const PandarShimBridge*, void*, void*, PluginConnectionResult
);
void pandar_plugin_shim_dispatch_offline_deliveries(
    const PandarShimBridge*, void*, void*, const uint64_t*, std::size_t
);

using PluginConnectionPrinterVisitor = void (*)(
    void*,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    int32_t
);
using PluginConnectionDeviceVisitor = void (*)(void*, const uint8_t*, std::size_t, uint64_t);

int32_t pandar_plugin_printer_refresh_session_update(
    void*,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t
);
PluginConnectionResult pandar_plugin_connection_refresh(void*);
int32_t pandar_plugin_connection_is_connected(void*);
using PluginDispatchWake = void (*)(void*);
int32_t pandar_plugin_connection_set_dispatch_waker(
    void*, void*, PluginDispatchWake
);
int32_t pandar_plugin_connection_set_account_epoch(void*, uint64_t);
PluginConnectionResult pandar_plugin_connection_take_transition(void*);

int32_t pandar_plugin_connection_visit_printers(
    void*, void*, PluginConnectionPrinterVisitor
);
int32_t pandar_plugin_connection_printer_eligible(
    void*, const uint8_t*, std::size_t
);
int32_t pandar_plugin_connection_take_offline(
    void*, void*, PluginConnectionDeviceVisitor
);
PluginHttpResult pandar_plugin_connection_take_stream_error(void*);
int32_t pandar_plugin_printer_refresh_session_set_tenant(
    void*, const uint8_t*, std::size_t
);
} // extern "C"

void shim_wake_status_dispatcher(void* context) {
    auto* agent = static_cast<Agent*>(context);
    agent->status_wake_generation.fetch_add(1, std::memory_order_release);
    agent->status_thread_wake.notify_all();
}

void shim_gate_lock(void* context) {
    static_cast<Agent*>(context)->callback_mutex.lock();
}

void shim_gate_unlock(void* context) {
    static_cast<Agent*>(context)->callback_mutex.unlock();
}

int32_t shim_status_thread_stopped(void* context) {
    return static_cast<Agent*>(context)->status_thread_stop.load() ? 1 : 0;
}

int32_t shim_invoke_server_connected(void* context, int32_t event, int32_t code) {
    auto* agent = static_cast<Agent*>(context);
    BBL::OnServerConnectedFn callback;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        callback = agent->on_server_connected;
    }
    if (!callback) return 0;
    callback(event, code);
    return 1;
}

int32_t shim_invoke_message(
    void* context,
    int32_t kind,
    const uint8_t* dev_id, std::size_t dev_id_len,
    const uint8_t* body, std::size_t body_len
) {
    auto* agent = static_cast<Agent*>(context);
    BBL::OnMessageFn callback;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        callback = kind == 1 ? agent->on_message : agent->on_local_message;
    }
    if (!callback) return 0;
    callback(
        std::string(reinterpret_cast<const char*>(dev_id), dev_id_len),
        std::string(reinterpret_cast<const char*>(body), body_len)
    );
    return 1;
}

int32_t shim_invoke_printer_connected(
    void* context, const uint8_t* body, std::size_t body_len
) {
    auto* agent = static_cast<Agent*>(context);
    BBL::OnPrinterConnectedFn callback;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        callback = agent->on_printer_connected;
    }
    if (!callback) return 0;
    callback(std::string(reinterpret_cast<const char*>(body), body_len));
    return 1;
}

int32_t shim_invoke_firmware_status(
    void* context, int32_t kind, const uint8_t* dev_id, std::size_t dev_id_len
) {
    auto* agent = static_cast<Agent*>(context);
    auto result = pandar_plugin_firmware_next_status_override(
        agent->firmware_session(), dev_id, dev_id_len
    );
    auto body = body_from_result(result);
    if (result.status != 0) return 0;
    return shim_invoke_message(
        context,
        kind,
        dev_id,
        dev_id_len,
        reinterpret_cast<const uint8_t*>(body.data()),
        body.size()
    );
}

int32_t shim_invoke_local_connected(
    void* context, int32_t state, const uint8_t* dev_id, std::size_t dev_id_len
) {
    auto* agent = static_cast<Agent*>(context);
    BBL::OnLocalConnectedFn callback;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        callback = agent->on_local_connect;
    }
    if (!callback) return 0;
    callback(state, std::string(reinterpret_cast<const char*>(dev_id), dev_id_len), {});
    return 1;
}

const PandarShimBridge kShimBridge = {
    shim_gate_lock,
    shim_gate_unlock,
    shim_status_thread_stopped,
    shim_invoke_server_connected,
    shim_invoke_message,
    shim_invoke_local_connected,
    shim_invoke_printer_connected,
    shim_invoke_firmware_status,
};

bool connection_printer_eligible_under_refresh(
    const Agent* agent,
    const std::string& dev_id
) {
    return agent && pandar_plugin_connection_printer_eligible(
        agent->connection_session(),
        reinterpret_cast<const uint8_t*>(dev_id.data()),
        dev_id.size()
    ) != 0;
}

bool connection_printer_eligible(const Agent* agent, const std::string& dev_id) {
    if (!agent) return false;
    std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
    return connection_printer_eligible_under_refresh(agent, dev_id);
}

struct IssuedOfflineDelivery {
    std::string dev_id;
    std::uint64_t ticket = 0;
};

extern "C" void collect_offline_device(
    void* context,
    const uint8_t* ptr,
    std::size_t len,
    std::uint64_t ticket
) {
    static_cast<std::vector<IssuedOfflineDelivery>*>(context)->push_back({
        std::string(reinterpret_cast<const char*>(ptr), len),
        ticket,
    });
}

} // namespace pandar::network_plugin
