#pragma once

namespace pandar::network_plugin {

struct StatusHeartbeatTarget {
    MessageTunnel tunnel = MessageTunnel::Cloud;
    std::string dev_id;
    std::uint64_t generation = 0;
};

inline void collect_status_heartbeat_target(
    void* context,
    int32_t tunnel,
    const uint8_t* dev_id,
    std::size_t dev_id_len,
    uint64_t generation
) {
    static_cast<std::vector<StatusHeartbeatTarget>*>(context)->push_back({
        tunnel == kStudioCloudTunnel ? MessageTunnel::Cloud : MessageTunnel::Local,
        std::string(reinterpret_cast<const char*>(dev_id), dev_id_len),
        generation,
    });
}

PluginStudioHeartbeatPlan status_heartbeat_plan(
    Agent* agent,
    std::vector<StatusHeartbeatTarget>* targets = nullptr
) {
    return pandar_plugin_studio_heartbeat_plan(
        agent->printer_refresh_session,
        targets,
        targets ? collect_status_heartbeat_target : nullptr
    );
}

void start_status_heartbeat(Agent* agent) {
    if (!agent || agent->status_thread.joinable()) return;
    agent->status_thread_stop = false;
    agent->status_thread = std::thread([agent] {
        while (!agent->status_thread_stop.load()) {
            const auto wait = status_heartbeat_plan(agent);
            std::unique_lock<std::mutex> wait_lock(agent->status_thread_mutex);
            if (agent->status_thread_wake.wait_for(
                    wait_lock,
                    std::chrono::milliseconds(wait.wait_ms),
                    [agent] { return agent->status_thread_stop.load(); }
                )) break;
            wait_lock.unlock();
            const auto recovered = try_no_auth_session(agent, false);
            if (agent->status_thread_stop.load()) break;
            if (recovered && !refresh_printer_status_cache(agent)) continue;
            std::vector<StatusHeartbeatTarget> targets;
            const auto plan = status_heartbeat_plan(agent, &targets);
            if (plan.refresh != 0 && !refresh_printer_status_cache(agent)) continue;
            if (agent->status_thread_stop.load()) break;
            for (const auto& target : targets) {
                if (agent->status_thread_stop.load()) break;
                if (target.tunnel == MessageTunnel::Cloud) {
                    emit_cloud_printer_connected_status(agent, target.dev_id);
                } else {
                    emit_printer_status(
                        agent, target.dev_id, target.tunnel, target.generation
                    );
                }
            }
        }
    });
}

void stop_status_heartbeat(Agent* agent) {
    if (!agent) return;
    agent->status_thread_stop = true;
    agent->status_thread_wake.notify_all();
    if (agent->status_thread.joinable()) agent->status_thread.join();
}

} // namespace pandar::network_plugin
