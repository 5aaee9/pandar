#pragma once

#include "shim_connection.hpp"

void refresh_local_webserver_config(void*);

namespace pandar::network_plugin {

void refresh_local_webserver_config(Agent* agent);
bool try_no_auth_session(Agent* agent, bool initial_attempt);
void dispatch_http_error(Agent* agent, unsigned code, const std::string& body);
void trace_plugin_event(const Agent* agent, const std::string& message);

extern "C" {

struct PluginDispatchBridge {
    PandarShimBridge base;
    int32_t (*firmware_generation_current)(void*, uint64_t);
    int32_t (*gate_try_lock_until)(void*, uint64_t);
    uint64_t (*steady_tick_ns)(void*);
    uint64_t (*now_ms)(void*);
    void (*refresh_local_webserver)(void*);
    void (*trace)(void*, const uint8_t*, std::size_t);
    void (*invoke_http_error)(void*, uint32_t, const uint8_t*, std::size_t);
    int32_t (*logged_out)(void*);
    int32_t (*sync_firmware)(void*, void*);
    void (*retry_no_auth)(void*);
    int32_t (*invoke_local_connected_with_body)(
        void*, int32_t, const uint8_t*, std::size_t, const uint8_t*, std::size_t
    );
};

struct PluginDispatchMessageRequest {
    void* session;
    void* firmware_session;
    uint64_t firmware_generation;
    int32_t tunnel;
    uint64_t local_generation;
    const uint8_t* dev_id_ptr;
    std::size_t dev_id_len;
    const uint8_t* message_ptr;
    std::size_t message_len;
};

struct PluginPendingOutcome {
    uint32_t wait_ms;
    int32_t logged_out;
};

int32_t pandar_plugin_dispatch_message(
    const PluginDispatchBridge*, void*, PluginDispatchMessageRequest
);
int32_t pandar_plugin_dispatch_connect_local(
    const PluginDispatchBridge*, void*, void*, const uint8_t*, std::size_t
);
int32_t pandar_plugin_dispatch_firmware_callback(
    const PluginDispatchBridge*, void*, void*, void*
);
PluginPendingOutcome pandar_plugin_dispatch_pending(
    const PluginDispatchBridge*, void*, void*, void*, int32_t
);
void pandar_plugin_dispatch_refresh_drain(
    const PluginDispatchBridge*, void*, void*, PluginConnectionResult,
    const uint64_t*, std::size_t
);

} // extern "C"

int32_t shim_dispatch_firmware_generation_current(void* context, uint64_t expected) {
    auto* agent = static_cast<Agent*>(context);
    return pandar_plugin_firmware_session_generation_current(
        agent->firmware_session, expected
    );
}

int32_t shim_dispatch_gate_try_lock_until(void* context, uint64_t deadline_ns) {
    auto* agent = static_cast<Agent*>(context);
    const auto deadline = std::chrono::steady_clock::time_point(
        std::chrono::duration_cast<std::chrono::steady_clock::duration>(
            std::chrono::nanoseconds(static_cast<std::chrono::nanoseconds::rep>(deadline_ns))
        )
    );
    return agent->callback_mutex.try_lock_until(deadline) ? 1 : 0;
}

uint64_t shim_dispatch_steady_tick_ns(void*) {
    return static_cast<uint64_t>(std::chrono::duration_cast<std::chrono::nanoseconds>(
        std::chrono::steady_clock::now().time_since_epoch()
    ).count());
}

uint64_t shim_dispatch_now_ms(void*) {
    return static_cast<uint64_t>(std::chrono::duration_cast<std::chrono::milliseconds>(
        std::chrono::steady_clock::now().time_since_epoch()
    ).count());
}

void shim_dispatch_refresh_local_webserver(void* context) {
    refresh_local_webserver_config(static_cast<Agent*>(context));
}

void shim_dispatch_trace(void* context, const uint8_t* message, std::size_t len) {
    trace_plugin_event(
        static_cast<Agent*>(context),
        std::string(reinterpret_cast<const char*>(message), len)
    );
}

void shim_dispatch_invoke_http_error(
    void* context, uint32_t code, const uint8_t* body, std::size_t len
) {
    dispatch_http_error(
        static_cast<Agent*>(context),
        code,
        std::string(reinterpret_cast<const char*>(body), len)
    );
}

int32_t shim_dispatch_logged_out(void* context) {
    auto* agent = static_cast<Agent*>(context);
    std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
    return agent->token.empty() ? 1 : 0;
}

int32_t shim_dispatch_sync_firmware(void* context, void*) {
    auto* agent = static_cast<Agent*>(context);
    if (!agent->firmware_session) return 0;
    std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
    pandar_plugin_connection_sync_firmware(
        agent->printer_refresh_session,
        agent->firmware_session,
        pandar_plugin_firmware_session_generation(agent->firmware_session),
        agent->firmware_observation_sequence.fetch_add(1, std::memory_order_relaxed) + 1
    );
    return 0;
}

void shim_dispatch_retry_no_auth(void* context) {
    try_no_auth_session(static_cast<Agent*>(context), false);
}

int32_t shim_invoke_local_connected_with_body(
    void* context,
    int32_t state,
    const uint8_t* dev_id,
    std::size_t dev_id_len,
    const uint8_t* body,
    std::size_t body_len
) {
    auto* agent = static_cast<Agent*>(context);
    BBL::OnLocalConnectedFn callback;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        callback = agent->on_local_connect;
    }
    if (!callback) return 0;
    callback(
        state,
        std::string(reinterpret_cast<const char*>(dev_id), dev_id_len),
        std::string(reinterpret_cast<const char*>(body), body_len)
    );
    return 1;
}

const PluginDispatchBridge kDispatchBridge = {
    kShimBridge,
    shim_dispatch_firmware_generation_current,
    shim_dispatch_gate_try_lock_until,
    shim_dispatch_steady_tick_ns,
    shim_dispatch_now_ms,
    shim_dispatch_refresh_local_webserver,
    shim_dispatch_trace,
    shim_dispatch_invoke_http_error,
    shim_dispatch_logged_out,
    shim_dispatch_sync_firmware,
    shim_dispatch_retry_no_auth,
    shim_invoke_local_connected_with_body,
};

std::uint64_t current_firmware_generation(Agent* agent) {
    return pandar_plugin_firmware_session_generation(agent->firmware_session);
}

int dispatch_studio_message(
    Agent* agent,
    const std::string& dev_id,
    const std::string& message,
    MessageTunnel tunnel,
    std::uint64_t local_generation
) {
    PluginDispatchMessageRequest request{
        agent->printer_refresh_session,
        agent->firmware_session,
        current_firmware_generation(agent),
        tunnel == MessageTunnel::Cloud ? 0 : 1,
        local_generation,
        reinterpret_cast<const uint8_t*>(dev_id.data()), dev_id.size(),
        reinterpret_cast<const uint8_t*>(message.data()), message.size(),
    };
    return pandar_plugin_dispatch_message(&kDispatchBridge, agent, request);
}

int dispatch_connect_printer_local(Agent* agent, const std::string& dev_id) {
    return pandar_plugin_dispatch_connect_local(
        &kDispatchBridge,
        agent,
        agent->printer_refresh_session,
        reinterpret_cast<const uint8_t*>(dev_id.data()), dev_id.size()
    );
}

int dispatch_firmware_callback(Agent* agent) {
    return pandar_plugin_dispatch_firmware_callback(
        &kDispatchBridge,
        agent,
        agent->printer_refresh_session,
        agent->firmware_session
    );
}

PluginPendingOutcome dispatch_pending(Agent* agent, bool no_auth_retry_due) {
    return pandar_plugin_dispatch_pending(
        &kDispatchBridge,
        agent,
        agent->printer_refresh_session,
        agent->firmware_session,
        no_auth_retry_due ? 1 : 0
    );
}

/// Drains transitions, offline deliveries, queued Studio work, and the
/// pending stream error after an inline session mutation.
void dispatch_pending_deliveries(Agent* agent) {
    dispatch_pending(agent, false);
}

/// Drains the transitions, offline tickets, queued Studio work, and pending
/// stream error collected by one printer-refresh transaction.
void dispatch_refresh_deliveries(
    Agent* agent,
    const PluginConnectionResult& transition,
    const std::vector<IssuedOfflineDelivery>& offline
) {
    if (!agent) return;
    std::vector<std::uint64_t> tickets;
    tickets.reserve(offline.size());
    for (const auto& issued : offline) {
        tickets.push_back(issued.ticket);
    }
    pandar_plugin_dispatch_refresh_drain(
        &kDispatchBridge,
        agent,
        agent->printer_refresh_session,
        transition,
        tickets.data(),
        tickets.size()
    );
}

std::uint64_t current_local_printer_generation(const Agent* agent, const std::string& dev_id) {
    if (!agent) return 0;
    return pandar_plugin_studio_local_generation(
        agent->printer_refresh_session,
        reinterpret_cast<const uint8_t*>(dev_id.data()), dev_id.size()
    );
}

} // namespace pandar::network_plugin
