#pragma once

#include <algorithm>

#include "shim_types.hpp"

namespace pandar::network_plugin {

void finalize_agent_destroy(Agent*);

inline thread_local std::vector<Agent*> active_agent_leases;

class AgentCallLease {
public:
    explicit AgentCallLease(Agent* agent)
        : agent_(agent && agent->lifetime.acquire() ? agent : nullptr) {
        if (agent_) active_agent_leases.push_back(agent_);
    }
    AgentCallLease(const AgentCallLease&) = delete;
    AgentCallLease& operator=(const AgentCallLease&) = delete;
    ~AgentCallLease() {
        if (!agent_) return;
        active_agent_leases.pop_back();
        if (agent_->lifetime.release()) {
            if (agent_->on_worker_thread()) {
                auto* deferred = agent_;
                std::thread([deferred] { finalize_agent_destroy(deferred); }).detach();
            } else {
                finalize_agent_destroy(agent_);
            }
        }
    }
    explicit operator bool() const { return agent_ != nullptr; }

    static bool held_by_current_thread(const Agent* agent) {
        return std::find(active_agent_leases.begin(), active_agent_leases.end(), agent) !=
            active_agent_leases.end();
    }

private:
    Agent* agent_;
};

class AgentAccess {
public:
    explicit AgentAccess(void* raw)
        : agent_(reinterpret_cast<Agent*>(raw)), lease_(agent_) {
        if (!lease_) agent_ = nullptr;
    }
    Agent* operator->() const { return agent_; }
    operator Agent*() const { return agent_; }
    explicit operator bool() const { return agent_ != nullptr; }

private:
    Agent* agent_;
    AgentCallLease lease_;
};

} // namespace pandar::network_plugin
