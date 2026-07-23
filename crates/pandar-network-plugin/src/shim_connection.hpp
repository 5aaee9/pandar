#pragma once

#include "shim_studio_session.hpp"

namespace pandar::network_plugin {

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

using PluginConnectionPrinterVisitor = void (*)(
    void*,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t,
    int32_t
);
using PluginConnectionDeviceVisitor = void (*)(void*, const uint8_t*, std::size_t, uint64_t);

void* pandar_plugin_printer_refresh_session_create(
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t
);
int32_t pandar_plugin_printer_refresh_session_update(
    void*,
    const uint8_t*, std::size_t,
    const uint8_t*, std::size_t
);
PluginConnectionResult pandar_plugin_connection_refresh(void*);
int32_t pandar_plugin_connection_is_connected(void*);
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
int32_t pandar_plugin_connection_claim_delivery(void*, uint64_t);
void pandar_plugin_printer_refresh_session_destroy(void*);

} // extern "C"

PluginConnectionResult take_connection_transition(Agent* agent) {
    if (!agent) return {};
    std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
    return pandar_plugin_connection_take_transition(agent->printer_refresh_session);
}

void dispatch_connection_transition(Agent* agent, const PluginConnectionResult& result) {
    if (!agent || (!result.changed && !result.auth_changed)) return;
    if (result.changed) {
        std::lock_guard<std::recursive_timed_mutex> gate(agent->callback_mutex);
        BBL::OnServerConnectedFn callback;
        bool claimed = false;
        {
            std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
            std::lock_guard<std::mutex> lock(agent->status_mutex);
            if (!agent->status_thread_stop.load() &&
                pandar_plugin_connection_claim_delivery(
                    agent->printer_refresh_session,
                    result.transition_ticket
                ) == 1) {
                callback = agent->on_server_connected;
                claimed = true;
            }
        }
        if (claimed && callback) {
            callback(
                result.connected ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED,
                0
            );
        }
    }
    if (result.auth_changed) {
        std::lock_guard<std::recursive_timed_mutex> gate(agent->callback_mutex);
        BBL::OnServerConnectedFn callback;
        {
            std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
            std::lock_guard<std::mutex> lock(agent->status_mutex);
            if (agent->status_thread_stop.load() ||
                pandar_plugin_connection_claim_delivery(
                    agent->printer_refresh_session,
                    result.auth_ticket
                ) != 1) return;
            callback = agent->on_server_connected;
        }
        if (callback && result.auth_rejected) callback(5, 0);
    }
}

bool connection_printer_eligible_under_refresh(
    const Agent* agent,
    const std::string& dev_id
) {
    return agent && pandar_plugin_connection_printer_eligible(
        agent->printer_refresh_session,
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

struct IssuedStudioDelivery {
    int32_t kind = 0;
    int32_t state = 0;
    std::uint64_t ticket = 0;
    std::uint64_t generation = 0;
    std::string dev_id;
    std::string body;
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

std::vector<IssuedOfflineDelivery> take_printer_offline_transitions(Agent* agent) {
    std::vector<IssuedOfflineDelivery> issued;
    if (!agent) return issued;
    {
        std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
        pandar_plugin_connection_take_offline(
            agent->printer_refresh_session,
            &issued,
            collect_offline_device
        );
    }
    return issued;
}

std::vector<IssuedStudioDelivery> take_studio_offline_transitions(Agent* agent) {
    std::vector<IssuedStudioDelivery> deliveries;
    if (!agent) return deliveries;
    auto collect = [](void* context, int32_t kind, int32_t state,
                      uint64_t ticket, uint64_t generation,
                      const uint8_t* dev_id, std::size_t dev_id_len,
                      const uint8_t* body, std::size_t body_len) {
        static_cast<std::vector<IssuedStudioDelivery>*>(context)->push_back({
            kind,
            state,
            ticket,
            generation,
            std::string(reinterpret_cast<const char*>(dev_id), dev_id_len),
            std::string(reinterpret_cast<const char*>(body), body_len),
        });
    };
    pandar_plugin_studio_take_work(
        agent->printer_refresh_session, &deliveries, collect
    );
    return deliveries;
}

void dispatch_issued_printer_offline_transitions(
    Agent* agent,
    std::vector<IssuedOfflineDelivery> issued,
    std::vector<IssuedStudioDelivery> deliveries
) {
    if (!agent) return;
    for (const auto& offline : issued) {
        pandar_plugin_connection_claim_delivery(
            agent->printer_refresh_session, offline.ticket
        );
    }
    for (const auto& delivery : deliveries) {
        std::lock_guard<std::recursive_timed_mutex> gate(agent->callback_mutex);
        if (pandar_plugin_studio_claim_delivery(
                agent->printer_refresh_session, delivery.ticket
            ) != 1) continue;
        bool delivered = false;
        if (delivery.kind == 1 || delivery.kind == 2) {
            BBL::OnMessageFn callback;
            {
                std::lock_guard<std::mutex> lock(agent->status_mutex);
                callback = delivery.kind == 1
                    ? agent->on_message
                    : agent->on_local_message;
            }
            if (callback) {
                callback(delivery.dev_id, delivery.body);
                delivered = true;
            }
        } else if (delivery.kind == 3) {
            BBL::OnLocalConnectedFn callback;
            {
                std::lock_guard<std::mutex> lock(agent->status_mutex);
                callback = agent->on_local_connect;
            }
            if (callback) {
                callback(delivery.state, delivery.dev_id, {});
                delivered = true;
            }
        }
        pandar_plugin_studio_complete_delivery(
            agent->printer_refresh_session, delivery.ticket, delivered
        );
    }
}

void dispatch_printer_offline_transitions(
    Agent* agent,
    std::vector<IssuedOfflineDelivery> issued
) {
    dispatch_issued_printer_offline_transitions(
        agent, std::move(issued), take_studio_offline_transitions(agent)
    );
}

} // namespace pandar::network_plugin
