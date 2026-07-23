#pragma once

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <functional>
#include <iostream>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

#if defined(_WIN32)
#include <windows.h>
#else
#include <dlfcn.h>
#endif

namespace BBL {
using OnMessageFn = std::function<void(std::string, std::string)>;
using OnLocalConnectedFn = std::function<void(int, std::string, std::string)>;
}

namespace {

constexpr int kSuccess = 0;
constexpr int kConnectFailed = -2;
constexpr int kInvalidResult = -19;

bool contains(const std::string& body, const std::string& value) {
    return body.find(value) != std::string::npos;
}

struct Library {
#if defined(_WIN32)
    HMODULE handle;
#else
    void* handle;
#endif
    explicit Library(const char* path)
#if defined(_WIN32)
        : handle(LoadLibraryA(path)) {}
#else
        : handle(dlopen(path, RTLD_NOW | RTLD_LOCAL)) {}
#endif
    ~Library() {
#if defined(_WIN32)
        if (handle) FreeLibrary(handle);
#else
        if (handle) dlclose(handle);
#endif
    }
    template <class T> T sym(const char* name) const {
#if defined(_WIN32)
        auto* raw = handle ? reinterpret_cast<void*>(GetProcAddress(handle, name)) : nullptr;
#else
        auto* raw = handle ? dlsym(handle, name) : nullptr;
#endif
        if (!raw) {
            std::cerr << "missing symbol: " << name << "\n";
            std::exit(3);
        }
        return reinterpret_cast<T>(raw);
    }
};

using Clock = std::chrono::steady_clock;

struct Capture {
    std::atomic<int> active{0};
    std::atomic<bool> concurrent{false};
    std::atomic<int> delayed_callbacks{0};
    std::atomic<int> overlap_callbacks{0};
    std::atomic<int> deadline_callbacks{0};
    std::atomic<int> forbidden_callbacks{0};
    std::atomic<int> stale_local_generation_callbacks{0};
    std::atomic<int> firmware_status_callbacks{0};
    std::atomic<int> firmware_status_callbacks_after_logout{0};
    std::atomic<bool> auxiliary_fence_new{false};
    std::atomic<bool> auxiliary_fence_old{false};
    std::atomic<bool> version_heartbeat_committed{false};
    std::atomic<bool> reentrant_done{false};
    std::atomic<bool> synchronous_reentrant_done{false};
    std::atomic<bool> status_logout_armed{false};
    std::atomic<bool> status_logout_entered{false};
    std::atomic<bool> status_logout_completed{false};
    std::atomic<bool> status_deadline_armed{false};
    std::atomic<bool> status_deadline_entered{false};
    std::mutex mutex;
    std::condition_variable ready;
    bool delayed_entered = false;
    bool cloud_version = false;
    bool local_version = false;
    bool overlap_version = false;
    bool overlap_exact = false;
    bool rejection_exact = false;
    Clock::time_point delayed_at{};
    Clock::time_point overlap_at{};
    std::function<void()> reentrant_logout;
    std::function<void()> synchronous_reentrant_logout;
    std::function<void()> status_logout;

    void on_message(bool cloud, const std::string& dev_id, const std::string& body) {
        if (active.fetch_add(1) != 0) concurrent = true;
        if (dev_id != "studio-serial-1") concurrent = true;
        if (contains(body, R"("command":"push_status")") && status_logout_armed.exchange(false)) {
            status_logout_entered = true;
            ready.notify_all();
            std::this_thread::sleep_for(std::chrono::milliseconds(1'400));
            if (status_logout) status_logout();
            status_logout_completed = true;
            ready.notify_all();
        }
        if (contains(body, R"("command":"push_status")") && status_deadline_armed.exchange(false)) {
            status_deadline_entered = true;
            ready.notify_all();
            std::this_thread::sleep_for(std::chrono::milliseconds(2'200));
        }
        if (contains(body, R"("sequence_id":"c-version")")) {
            std::lock_guard<std::mutex> lock(mutex);
            cloud_version = cloud && version_exact(body, "c-version");
        } else if (contains(body, R"("sequence_id":"l-version")")) {
            std::lock_guard<std::mutex> lock(mutex);
            local_version = !cloud && version_exact(body, "l-version");
        } else if (contains(body, R"("sequence_id":"c-lock-overlap-version")")) {
            std::lock_guard<std::mutex> lock(mutex);
            overlap_version = cloud && version_exact(body, "c-lock-overlap-version");
        } else if (contains(body, R"("sequence_id":"c-delay-reject")")) {
            {
                std::lock_guard<std::mutex> lock(mutex);
                rejection_exact = cloud &&
                    contains(body, R"("command":"mc_for_ams_firmware_upgrade")") &&
                    contains(body, R"("result":"fail")") &&
                    contains(body, R"("err_code":765)") &&
                    contains(body, R"("reason":"printer_busy")") &&
                    contains(body, R"("message":"selector rejected")") &&
                    contains(body, R"("status":"FAIL")") &&
                    contains(body, R"("progress":"42")") &&
                    contains(body, R"("cfg":"101")");
                delayed_at = Clock::now();
                delayed_entered = true;
            }
            ++delayed_callbacks;
            ready.notify_all();
            std::this_thread::sleep_for(std::chrono::milliseconds(300));
        } else if (contains(body, R"("sequence_id":"c-lock-overlap-ack")")) {
            {
                std::lock_guard<std::mutex> lock(mutex);
                overlap_at = Clock::now();
                overlap_exact = cloud &&
                    contains(body, R"("command":"upgrade_confirm")") &&
                    contains(body, R"("result":"fail")") &&
                    contains(body, R"("err_code":765)") &&
                    contains(body, R"("reason":"printer_busy")");
            }
            ++overlap_callbacks;
            ready.notify_all();
        } else if (contains(body, R"("sequence_id":"c-deadline")")) {
            ++deadline_callbacks;
        } else if (contains(body, R"("sequence_id":"c-reentrant")")) {
            if (reentrant_logout) reentrant_logout();
        } else if (contains(body, R"("sequence_id":"c-synchronous-reentrant")")) {
            if (synchronous_reentrant_logout) synchronous_reentrant_logout();
        } else if (contains(body, R"("sequence_id":"c-generation-fence")")) {
            ++forbidden_callbacks;
        } else if (contains(body, R"("sequence_id":"l-generation-fence")")) {
            ++stale_local_generation_callbacks;
        } else if (contains(body, R"("sequence_id":"c-logout")") ||
                   contains(body, R"("sequence_id":"c-lock-order")") ||
                   contains(body, R"("sequence_id":"c-destroy")")) {
            ++forbidden_callbacks;
        }
        if (contains(body, R"("upgrade_state":{"status":"UPGRADING","progress":"37"})") &&
            contains(body, R"("cfg":"101")")) {
            ++firmware_status_callbacks;
        }
        if (contains(body, R"("command":"push_status")") && contains(body, "08.08.08.08")) {
            auxiliary_fence_new = true;
        }
        if (contains(body, R"("command":"push_status")") && contains(body, "06.06.06.06")) {
            auxiliary_fence_old = true;
        }
        if (status_logout_completed.load() &&
            contains(body, R"("sequence_id":"0")") &&
            contains(body, R"("result":"fail")")) {
            ++firmware_status_callbacks_after_logout;
        }
        const bool version_heartbeat = !cloud &&
            contains(body, R"("command":"push_status")") &&
            contains(body, "09.87.65.43");
        active.fetch_sub(1);
        if (version_heartbeat) version_heartbeat_committed = true;
    }

    static bool version_exact(const std::string& body, const std::string& sequence) {
        if (!contains(body, R"("sequence_id":")" + sequence + R"(")") ||
            contains(body, "01.07.00.00") || contains(body, "01.07.22.25")) return false;
        const std::vector<std::string> fields = {
            R"("name":"ota")", R"("sw_ver":"01.02.03.04")", R"("sw_new_ver":"01.02.04.00")",
            R"("new_ver":"01.02.05.00")", R"("visible":true)", R"("product_name":"Main")",
            R"("sn":"SERIAL")", R"("hw_ver":"AP05")", R"("flag":5)",
            R"("name":"ams/0")", R"("sw_ver":"02.00.00.00")", R"("sw_new_ver":"02.00.01.00")", R"("new_ver":"02.00.02.00")", R"("visible":false)", R"("product_name":"AMS")", R"("sn":"AMS0")", R"("hw_ver":"AMS01")", R"("flag":1)",
            R"("name":"n3f/0")", R"("sw_ver":"02.01.00.00")", R"("sw_new_ver":"02.01.01.00")", R"("new_ver":"02.01.02.00")", R"("product_name":"AMS 2 Pro")", R"("sn":"N3F0")", R"("hw_ver":"N3F01")", R"("flag":2)",
            R"("name":"n3s/0")", R"("sw_ver":"03.00.00.00")", R"("sw_new_ver":"03.00.01.00")", R"("new_ver":"03.00.02.00")", R"("product_name":"AMS-HT")", R"("sn":"N3S0")", R"("hw_ver":"N3S01")", R"("flag":3)",
            R"("name":"future/9")", R"("sw_ver":"09.09.09.09")", R"("sw_new_ver":"09.09.10.00")", R"("new_ver":"09.09.11.00")", R"("product_name":"Future")", R"("sn":"F9")", R"("hw_ver":"F09")", R"("flag":9)"
        };
        for (const auto& field : fields) if (!contains(body, field)) return false;
        return true;
    }
};

[[noreturn]] void fail(void* agent, int (*destroy)(void*), const std::string& message) {
    if (agent) destroy(agent);
    std::cerr << message << "\n";
    std::exit(2);
}

bool wait_until(const std::function<bool()>& predicate, Clock::time_point deadline) {
    while (!predicate() && Clock::now() < deadline) {
        std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
    return predicate();
}

std::string command(const std::string& name, const std::string& sequence) {
    if (name == "upgrade_confirm")
        return R"({"upgrade":{"command":"upgrade_confirm","sequence_id":")" + sequence + R"(","src_id":1}})";
    if (name == "consistency_confirm")
        return R"({"upgrade":{"command":"consistency_confirm","sequence_id":")" + sequence + R"(","src_id":2}})";
    if (name == "start")
        return R"({"upgrade":{"command":"start","sequence_id":")" + sequence +
            R"(","src_id":3,"url":"https://user:secret@example.invalid/fw.bin?sig=ABI_SENTINEL","module":"n3s/0","version":"03.04.05.06"}})";
    return R"({"upgrade":{"command":"mc_for_ams_firmware_upgrade","sequence_id":")" + sequence + R"(","src_id":4,"id":-7}})";
}

} // namespace
