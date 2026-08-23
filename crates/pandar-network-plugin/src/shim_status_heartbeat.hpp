#pragma once

namespace pandar::network_plugin {

void dispatch_http_error(Agent*, unsigned, const std::string&);

PluginStudioHeartbeatPlan status_heartbeat_plan(Agent* agent) {
    return pandar_plugin_studio_heartbeat_plan(
        agent->printer_refresh_session, nullptr, nullptr
    );
}

void sync_streamed_firmware(Agent* agent) {
    if (!agent || !agent->firmware_session) return;
    std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
    std::lock_guard<std::recursive_mutex> transition(agent->firmware_transition_mutex);
    pandar_plugin_connection_sync_firmware(
        agent->printer_refresh_session,
        agent->firmware_session,
        agent->firmware_generation,
        ++agent->firmware_observation_sequence
    );
}

void dispatch_pending_stream_error(Agent* agent) {
    auto result = pandar_plugin_connection_take_stream_error(
        agent->printer_refresh_session
    );
    if (result.status == 0) {
        body_from_result(result);
        return;
    }
    const auto code = result.http_code;
    auto body = body_from_result(result);
    dispatch_http_error(agent, code, body);
}

void start_status_heartbeat(Agent* agent) {
    if (!agent || agent->status_thread.joinable()) return;
    agent->status_thread_stop = false;
    agent->status_thread = std::thread([agent] {
        while (!agent->status_thread_stop.load()) {
            const auto observed = agent->status_wake_generation.load(
                std::memory_order_acquire
            );
            const auto plan = status_heartbeat_plan(agent);
            bool logged_out;
            {
                std::lock_guard<std::recursive_mutex> refresh(
                    agent->printer_refresh_mutex
                );
                logged_out = agent->token.empty();
            }
            bool no_auth_retry_due = false;
            if (plan.wait_ms != 0) {
                std::unique_lock<std::mutex> wait_lock(agent->status_thread_mutex);
                const auto woken = [agent, observed] {
                    return agent->status_thread_stop.load()
                        || agent->status_wake_generation.load(
                            std::memory_order_acquire
                        ) != observed;
                };
                if (logged_out) {
                    no_auth_retry_due = !agent->status_thread_wake.wait_for(
                        wait_lock, std::chrono::seconds(2), woken
                    );
                } else {
                    agent->status_thread_wake.wait(wait_lock, woken);
                }
            }
            if (agent->status_thread_stop.load()) break;

            sync_streamed_firmware(agent);
            dispatch_connection_transition(agent, take_connection_transition(agent));
            dispatch_printer_offline_transitions(
                agent, take_printer_offline_transitions(agent)
            );
            dispatch_pending_stream_error(agent);

            if (logged_out && no_auth_retry_due) try_no_auth_session(agent, false);
            if (agent->status_thread_stop.load()) break;

            dispatch_connection_transition(agent, take_connection_transition(agent));
            dispatch_printer_offline_transitions(
                agent, take_printer_offline_transitions(agent)
            );
            dispatch_pending_stream_error(agent);
        }
    });
}

void stop_status_heartbeat(Agent* agent) {
    if (!agent) return;
    agent->status_thread_stop = true;
    agent->status_wake_generation.fetch_add(1, std::memory_order_release);
    agent->status_thread_wake.notify_all();
    if (agent->status_thread.joinable()) agent->status_thread.join();
}

} // namespace pandar::network_plugin
