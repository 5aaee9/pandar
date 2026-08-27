#pragma once

#include "shim_dispatch.hpp"

namespace pandar::network_plugin {

void start_status_heartbeat(Agent* agent) {
    if (!agent || agent->status_thread.joinable()) return;
    agent->status_thread_stop = false;
    agent->status_thread = std::thread([agent] {
        bool no_auth_retry_due = false;
        while (!agent->status_thread_stop.load()) {
            const auto observed = agent->status_wake_generation.load(
                std::memory_order_acquire
            );
            const auto pending = dispatch_pending(agent, no_auth_retry_due);
            no_auth_retry_due = false;
            if (agent->status_thread_stop.load()) break;

            if (pending.wait_ms != 0) {
                std::unique_lock<std::mutex> wait_lock(agent->status_thread_mutex);
                const auto woken = [agent, observed] {
                    return agent->status_thread_stop.load()
                        || agent->status_wake_generation.load(
                            std::memory_order_acquire
                        ) != observed;
                };
                if (pending.logged_out != 0) {
                    no_auth_retry_due = !agent->status_thread_wake.wait_for(
                        wait_lock, std::chrono::seconds(2), woken
                    );
                } else {
                    agent->status_thread_wake.wait(wait_lock, woken);
                }
            }
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
