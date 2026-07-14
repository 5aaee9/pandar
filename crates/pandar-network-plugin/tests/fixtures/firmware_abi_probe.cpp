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
}

namespace {

constexpr int kSuccess = 0;

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
    std::atomic<int> firmware_status_callbacks{0};
    std::atomic<int> firmware_status_callbacks_after_logout{0};
    std::atomic<bool> auxiliary_fence_new{false};
    std::atomic<bool> auxiliary_fence_old{false};
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
        active.fetch_sub(1);
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

int main(int argc, char** argv) {
    if (argc != 3) return 2;
    Library lib(argv[1]);
    using create_fn = void* (*)(std::string);
    using agent_fn = int (*)(void*);
    using string_fn = int (*)(void*, std::string);
    using send_fn = int (*)(void*, std::string, std::string, int, int);
    using callback_fn = int (*)(void*, BBL::OnMessageFn);
    using print_info_fn = int (*)(void*, unsigned int*, std::string*);
    using get_string_fn = std::string (*)(void*);
    using catalog_fn = int (*)(void*, std::string, unsigned int*, std::string*);
    using logout_fn = int (*)(void*, bool);
    auto create = lib.sym<create_fn>("bambu_network_create_agent");
    auto destroy = lib.sym<agent_fn>("bambu_network_destroy_agent");
    auto start = lib.sym<agent_fn>("bambu_network_start");
    auto set_config = lib.sym<string_fn>("bambu_network_set_config_dir");
    auto change_user = lib.sym<string_fn>("bambu_network_change_user");
    auto get_print_info = lib.sym<print_info_fn>("bambu_network_get_user_print_info");
    auto get_selected_machine = lib.sym<get_string_fn>("bambu_network_get_user_selected_machine");
    auto get_catalog = lib.sym<catalog_fn>("bambu_network_get_printer_firmware");
    auto send_cloud = lib.sym<send_fn>("bambu_network_send_message");
    auto send_local = lib.sym<send_fn>("bambu_network_send_message_to_printer");
    auto set_cloud = lib.sym<callback_fn>("bambu_network_set_on_message_fn");
    auto set_local = lib.sym<callback_fn>("bambu_network_set_on_local_message_fn");
    auto logout = lib.sym<logout_fn>("bambu_network_user_logout");

    void* agent = create("firmware-probe");
    Capture capture;
    if (!agent || set_config(agent, argv[2]) != kSuccess || start(agent) != kSuccess) {
        fail(agent, destroy, "firmware probe setup failed");
    }
    unsigned code = 0;
    std::string body;
    unsigned delayed_code = 0;
    std::string delayed_body;
    int delayed_seed_rc = -1;
    std::thread delayed_seed([&] {
        delayed_seed_rc = get_print_info(agent, &delayed_code, &delayed_body);
    });
    const auto race_ready = std::filesystem::path(argv[2]) / "auxiliary-printer-ready";
    if (!wait_until([&] { return std::filesystem::exists(race_ready); }, Clock::now() + std::chrono::seconds(5))) {
        fail(agent, destroy, "delayed printer response did not enter mock Hub");
    }
    const auto selected = get_selected_machine(agent);
    std::filesystem::create_directory(std::filesystem::path(argv[2]) / "auxiliary-printer-applied");
    delayed_seed.join();
    if (set_cloud(agent, [&capture](std::string id, std::string body) { capture.on_message(true, id, body); }) != kSuccess ||
        set_local(agent, [&capture](std::string id, std::string body) { capture.on_message(false, id, body); }) != kSuccess) {
        fail(agent, destroy, "firmware callback setup failed");
    }
    const auto background_failure =
        std::filesystem::path(argv[2]) / "background-printer-failure-served";
    if (!wait_until(
            [&] { return std::filesystem::exists(background_failure); },
            Clock::now() + std::chrono::seconds(5)
        )) {
        fail(agent, destroy, "background printer failure was not served");
    }
    if (selected != "studio-serial-1" || delayed_seed_rc != kSuccess ||
        send_cloud(agent, "studio-serial-1", R"({"pushing":{"command":"pushall","sequence_id":"auxiliary-fence"}})", 0, 0) != kSuccess ||
        !capture.auxiliary_fence_new || capture.auxiliary_fence_old) {
        fail(agent, destroy, "newer auxiliary printer response did not fence delayed response");
    }
    if (get_print_info(agent, &code, &body) != kSuccess) fail(agent, destroy, "printer seed failed");

    if (get_catalog(agent, "studio-serial-1", &code, &body) != kSuccess || code != 200 ||
        body != R"({"devices":[{"dev_id":"studio-serial-1","firmware":[],"ams":[]}]})") {
        fail(agent, destroy, "empty firmware catalog was not exact: " + body);
    }
    if (get_catalog(agent, "studio-serial-1", &code, &body) != kSuccess ||
        body != R"({"devices":[{"dev_id":"studio-serial-1","firmware":[{"version":"01.02.04.00","url":"main.bin","description":"Main release"}],"ams":[{"firmware":[{"version":"03.01.00.00","url":"ams.bin","description":"AMS release"}]}]}]})") {
        fail(agent, destroy, "populated firmware catalog was not exact: " + body);
    }

    if (send_cloud(agent, "studio-serial-1", R"({"info":{"command":"get_version","sequence_id":"c-version"}})", 0, 0) != kSuccess ||
        send_local(agent, "studio-serial-1", R"({"info":{"command":"get_version","sequence_id":"l-version"}})", 0, 0) != kSuccess) {
        fail(agent, destroy, "firmware version refresh failed");
    }
    {
        std::lock_guard<std::mutex> lock(capture.mutex);
        if (!capture.cloud_version || !capture.local_version) fail(agent, destroy, "fresh versions were not exact");
    }

    std::atomic<bool> slow_version_returned{false};
    int slow_version_rc = -1;
    std::thread slow_version([&] {
        slow_version_rc = send_cloud(
            agent,
            "studio-serial-1",
            R"({"info":{"command":"get_version","sequence_id":"c-lock-overlap-version"}})",
            0,
            0
        );
        slow_version_returned = true;
    });
    const auto slow_version_entered =
        std::filesystem::path(argv[2]) / "slow-version-refresh-entered";
    if (!wait_until(
            [&] { return std::filesystem::exists(slow_version_entered); },
            Clock::now() + std::chrono::seconds(1)
        )) {
        std::cerr << "slow version refresh did not enter mock Hub\n";
        std::_Exit(2);
    }
    if (send_cloud(
            agent,
            "studio-serial-1",
            command("upgrade_confirm", "c-lock-overlap-ack"),
            0,
            0
        ) != kSuccess) {
        std::cerr << "overlapping firmware acknowledgement setup failed\n";
        std::_Exit(2);
    }
    const auto overlap_returned_at = Clock::now();
    std::this_thread::sleep_until(overlap_returned_at + std::chrono::milliseconds(1'000));
    if (capture.overlap_callbacks != 0) {
        std::cerr << "overlapping firmware acknowledgement entered Studio guard\n";
        std::_Exit(2);
    }
    const bool overlap_before_deadline = wait_until(
        [&] { return capture.overlap_callbacks.load() == 1; },
        overlap_returned_at + std::chrono::milliseconds(2'000)
    );
    const bool overlap_while_refreshing = !slow_version_returned.load();
    slow_version.join();
    long long overlap_delay_ms = 0;
    {
        std::lock_guard<std::mutex> lock(capture.mutex);
        overlap_delay_ms = std::chrono::duration_cast<std::chrono::milliseconds>(
            capture.overlap_at - overlap_returned_at
        ).count();
        if (!capture.overlap_version) fail(agent, destroy, "slow version refresh response changed");
        if (!capture.overlap_exact) fail(agent, destroy, "overlapping acknowledgement changed");
    }
    if (!overlap_before_deadline || capture.overlap_callbacks != 1 ||
        !overlap_while_refreshing || slow_version_rc != kSuccess ||
        overlap_delay_ms < 1'100 || overlap_delay_ms >= 2'000) {
        fail(agent, destroy, "firmware acknowledgement was lost behind slow version refresh");
    }

    const std::vector<std::pair<std::string, std::string>> cloud = {
        {"upgrade_confirm","c-upgrade"}, {"consistency_confirm","c-consistency"}, {"start","c-start"}
    };
    const std::vector<std::pair<std::string, std::string>> local = {
        {"upgrade_confirm","l-upgrade"}, {"consistency_confirm","l-consistency"},
        {"start","l-start"}, {"mc_for_ams_firmware_upgrade","l-switch"}
    };
    for (const auto& item : cloud)
        if (send_cloud(agent, "studio-serial-1", command(item.first, item.second), 0, 0) != kSuccess)
            fail(agent, destroy, "cloud firmware command failed");
    for (const auto& item : local)
        if (send_local(agent, "studio-serial-1", command(item.first, item.second), 0, 0) != kSuccess)
            fail(agent, destroy, "local firmware command failed");

    Clock::time_point returned_at;
    std::atomic<bool> send_returned{false};
    int delayed_rc = -1;
    std::thread delayed([&] {
        delayed_rc = send_cloud(agent, "studio-serial-1", command("mc_for_ams_firmware_upgrade", "c-delay-reject"), 0, 0);
        returned_at = Clock::now();
        send_returned = true;
    });
    std::this_thread::sleep_for(std::chrono::milliseconds(100));
    if (send_returned || capture.delayed_callbacks != 0 ||
        send_cloud(agent, "studio-serial-1", R"({"system":{"command":"unrelated"}})", 0, 0) != kSuccess) {
        fail(agent, destroy, "originating call was not delayed across unrelated send");
    }
    delayed.join();
    if (delayed_rc != kSuccess || capture.delayed_callbacks != 0) fail(agent, destroy, "firmware callback ran before return");
    std::this_thread::sleep_until(returned_at + std::chrono::milliseconds(1'000));
    if (capture.delayed_callbacks != 0) fail(agent, destroy, "firmware callback entered Studio guard");
    if (!wait_until([&] { return capture.delayed_callbacks.load() == 1; }, returned_at + std::chrono::milliseconds(2'000)))
        fail(agent, destroy, "firmware callback missed handoff deadline");
    long long delay_ms;
    {
        std::lock_guard<std::mutex> lock(capture.mutex);
        delay_ms = std::chrono::duration_cast<std::chrono::milliseconds>(capture.delayed_at - returned_at).count();
        if (!capture.rejection_exact) fail(agent, destroy, "rejected acknowledgement fields changed");
    }
    std::thread status([&] {
        send_cloud(agent, "studio-serial-1", R"({"pushing":{"command":"pushall","sequence_id":"status-overlap"}})", 0, 0);
    });
    status.join();
    if (capture.concurrent || capture.firmware_status_callbacks == 0) fail(agent, destroy, "callbacks were concurrent or lacked firmware status");

    capture.status_deadline_armed = true;
    std::thread status_deadline([&] {
        send_cloud(agent, "studio-serial-1", R"({"pushing":{"command":"pushall","sequence_id":"status-deadline"}})", 0, 0);
    });
    if (!wait_until([&] { return capture.status_deadline_entered.load(); }, Clock::now() + std::chrono::seconds(1)) ||
        send_cloud(agent, "studio-serial-1", command("upgrade_confirm", "c-deadline"), 0, 0) != kSuccess) {
        fail(agent, destroy, "deadline regression setup failed");
    }
    status_deadline.join();
    std::this_thread::sleep_for(std::chrono::milliseconds(300));
    if (capture.deadline_callbacks != 0) {
        fail(agent, destroy, "firmware callback entered after its return-anchored deadline");
    }

    const std::string profile = R"({"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"})";
    capture.synchronous_reentrant_logout = [&] {
        if (logout(agent, true) == kSuccess) capture.synchronous_reentrant_done = true;
    };
    if (send_cloud(
            agent,
            "studio-serial-1",
            R"({"info":{"command":"get_version","sequence_id":"c-synchronous-reentrant"}})",
            0,
            0
        ) != kSuccess ||
        !capture.synchronous_reentrant_done.load() ||
        change_user(agent, profile) != kSuccess ||
        get_print_info(agent, &code, &body) != kSuccess) {
        fail(agent, destroy, "synchronous firmware callback reentrant logout deadlocked");
    }
    capture.status_logout = [&] { logout(agent, true); };
    if (send_cloud(agent, "studio-serial-1", command("upgrade_confirm", "c-lock-order"), 0, 0) != kSuccess) {
        fail(agent, destroy, "lock-order regression setup failed");
    }
    capture.status_logout_armed = true;
    std::thread status_logout([&] {
        send_cloud(agent, "studio-serial-1", R"({"pushing":{"command":"pushall","sequence_id":"status-logout"}})", 0, 0);
    });
    if (!wait_until([&] { return capture.status_logout_entered.load(); }, Clock::now() + std::chrono::seconds(1))) {
        std::cerr << "status callback did not enter for generation fence\n";
        std::_Exit(2);
    }
    std::atomic<bool> version_fence_started{false};
    int version_fence_rc = -1;
    std::thread version_fence([&] {
        version_fence_started = true;
        version_fence_rc = send_cloud(
            agent,
            "studio-serial-1",
            R"({"info":{"command":"get_version","sequence_id":"c-generation-fence"}})",
            0,
            0
        );
    });
    if (!wait_until([&] { return version_fence_started.load(); }, Clock::now() + std::chrono::seconds(1)) ||
        !wait_until([&] { return capture.status_logout_completed.load(); }, Clock::now() + std::chrono::seconds(3))) {
        std::cerr << "status callback logout deadlocked against firmware dispatcher\n";
        std::_Exit(2);
    }
    status_logout.join();
    version_fence.join();
    if (version_fence_rc != kSuccess || capture.forbidden_callbacks != 0 ||
        capture.firmware_status_callbacks_after_logout != 0 ||
        change_user(agent, profile) != kSuccess ||
        get_print_info(agent, &code, &body) != kSuccess) {
        fail(agent, destroy, "generation fence did not cancel synchronous firmware callback");
    }

    if (send_cloud(agent, "studio-serial-1", command("upgrade_confirm", "c-logout"), 0, 0) != kSuccess ||
        logout(agent, true) != kSuccess) fail(agent, destroy, "logout cancellation setup failed");
    std::this_thread::sleep_for(std::chrono::milliseconds(2'100));
    const bool logout_cancelled = capture.forbidden_callbacks == 0;
    if (!logout_cancelled || change_user(agent, profile) != kSuccess || get_print_info(agent, &code, &body) != kSuccess) {
        fail(agent, destroy, "reentrant logout setup failed");
    }
    capture.reentrant_logout = [&] {
        if (logout(agent, true) == kSuccess) capture.reentrant_done = true;
    };
    if (send_cloud(agent, "studio-serial-1", command("upgrade_confirm", "c-reentrant"), 0, 0) != kSuccess ||
        !wait_until([&] { return capture.reentrant_done.load(); }, Clock::now() + std::chrono::milliseconds(2'100)) ||
        change_user(agent, profile) != kSuccess || get_print_info(agent, &code, &body) != kSuccess ||
        send_cloud(agent, "studio-serial-1", command("upgrade_confirm", "c-destroy"), 0, 0) != kSuccess) {
        fail(agent, destroy, "destroy cancellation setup failed");
    }
    destroy(agent);
    agent = nullptr;
    std::this_thread::sleep_for(std::chrono::milliseconds(2'100));
    const bool destroy_cancelled = capture.forbidden_callbacks == 0;
    if (!destroy_cancelled) {
        std::cerr << "callback ran after destroy\n";
        return 2;
    }
    std::cout << "{\"ok\":true,\"catalog_exact\":true,\"versions_exact\":true,"
              << "\"callback_delay_ms\":" << delay_ms
              << ",\"overlap_callback_delay_ms\":" << overlap_delay_ms
              << ",\"overlap_callback_exact\":true"
              << ",\"callbacks_serialized\":true,\"status_logout_safe\":true,"
              << "\"synchronous_generation_fenced\":true,"
              << "\"synchronous_reentrant_logout\":true,"
              << "\"deadline_expired\":true,"
              << "\"logout_cancelled\":true,\"destroy_cancelled\":true}\n";
    return 0;
}
