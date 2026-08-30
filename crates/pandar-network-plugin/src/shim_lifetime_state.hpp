#pragma once

#include <condition_variable>
#include <cstdint>
#include <mutex>

namespace pandar::network_plugin {

class AgentLifetime {
public:
    bool acquire() {
        std::lock_guard<std::mutex> lock(mutex_);
        if (destroy_requested_) return false;
        ++leases_;
        return true;
    }

    bool release() {
        std::lock_guard<std::mutex> lock(mutex_);
        if (leases_ > 0) --leases_;
        if (leases_ == 0) changed_.notify_all();
        return destroy_requested_ && leases_ == 0 && !finalizer_started_;
    }

    void request_destroy() {
        std::lock_guard<std::mutex> lock(mutex_);
        destroy_requested_ = true;
    }

    bool request_destroy_and_begin_finalizer() {
        std::lock_guard<std::mutex> lock(mutex_);
        destroy_requested_ = true;
        if (finalizer_started_) return false;
        finalizer_started_ = true;
        return true;
    }

    bool begin_finalizer() {
        std::lock_guard<std::mutex> lock(mutex_);
        if (finalizer_started_) return false;
        finalizer_started_ = true;
        return true;
    }

    void wait_for_leases() {
        std::unique_lock<std::mutex> lock(mutex_);
        changed_.wait(lock, [this] { return leases_ == 0; });
    }

private:
    std::mutex mutex_;
    std::condition_variable changed_;
    std::uint32_t leases_ = 0;
    bool destroy_requested_ = false;
    bool finalizer_started_ = false;
};

} // namespace pandar::network_plugin
