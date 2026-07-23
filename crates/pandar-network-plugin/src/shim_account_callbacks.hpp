#pragma once

#include "shim_types.hpp"

namespace pandar::network_plugin {

void enqueue_account_callback(Agent* agent, std::function<void()> callback) {
    if (!agent || !callback) return;
    std::lock_guard<std::mutex> lock(agent->account_callback_queue_mutex);
    agent->account_callback_queue.push_back(std::move(callback));
}

void drain_account_callbacks(Agent* agent) {
    if (!agent) return;
    {
        std::lock_guard<std::mutex> lock(agent->account_callback_queue_mutex);
        if (agent->account_callback_draining) return;
        agent->account_callback_draining = true;
    }
    while (true) {
        std::function<void()> callback;
        {
            std::lock_guard<std::mutex> lock(agent->account_callback_queue_mutex);
            if (agent->account_callback_queue.empty()) {
                agent->account_callback_draining = false;
                return;
            }
            callback = std::move(agent->account_callback_queue.front());
            agent->account_callback_queue.pop_front();
        }
        callback();
    }
}

} // namespace pandar::network_plugin
