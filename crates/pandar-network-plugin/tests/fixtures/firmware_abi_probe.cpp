#include "firmware_abi_probe_support.hpp"

int main(int argc, char** argv) {
    if (argc != 3) return 2;
    Library lib(argv[1]);
    using create_fn = void* (*)(std::string);
    using agent_fn = int (*)(void*);
    using string_fn = int (*)(void*, std::string);
    using send_fn = int (*)(void*, std::string, std::string, int, int);
    using callback_fn = int (*)(void*, BBL::OnMessageFn);
    using subscription_fn = int (*)(void*, std::vector<std::string>);
    using local_connect_callback_fn = int (*)(void*, BBL::OnLocalConnectedFn);
    using connect_printer_fn = int (*)(void*, std::string, std::string, std::string, std::string, bool);
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
    auto set_selected_machine = lib.sym<string_fn>("bambu_network_set_user_selected_machine");
    auto add_subscribe = lib.sym<subscription_fn>("bambu_network_add_subscribe");
    auto get_catalog = lib.sym<catalog_fn>("bambu_network_get_printer_firmware");
    auto send_cloud = lib.sym<send_fn>("bambu_network_send_message");
    auto send_local = lib.sym<send_fn>("bambu_network_send_message_to_printer");
    auto set_cloud = lib.sym<callback_fn>("bambu_network_set_on_message_fn");
    auto set_local = lib.sym<callback_fn>("bambu_network_set_on_local_message_fn");
    auto set_local_connect = lib.sym<local_connect_callback_fn>("bambu_network_set_on_local_connect_fn");
    auto connect_printer = lib.sym<connect_printer_fn>("bambu_network_connect_printer");
    auto disconnect_printer = lib.sym<agent_fn>("bambu_network_disconnect_printer");
    auto logout = lib.sym<logout_fn>("bambu_network_user_logout");

    void* agent = create("firmware-probe");
    Capture capture;
    const auto select_and_subscribe = [&] {
        return set_selected_machine(agent, "studio-serial-1") == kSuccess &&
            add_subscribe(agent, {"studio-serial-1"}) == kSuccess;
    };
    if (!agent || set_config(agent, argv[2]) != kSuccess || start(agent) != kSuccess) {
        fail(agent, destroy, "firmware probe setup failed");
    }
    unsigned code = 0;
    std::string body;
    if (get_print_info(agent, &code, &body) != kSuccess) {
        fail(agent, destroy, "auxiliary printer seed failed");
    }
    const auto selected_before = get_selected_machine(agent);
    if (!selected_before.empty() ||
        !select_and_subscribe()) {
        fail(agent, destroy, "selected machine getter performed implicit session work");
    }
    const auto selected = get_selected_machine(agent);
    if (set_cloud(agent, [&capture](std::string id, std::string body) { capture.on_message(true, id, body); }) != kSuccess ||
        set_local(agent, [&capture](std::string id, std::string body) { capture.on_message(false, id, body); }) != kSuccess ||
        set_local_connect(agent, [](int, std::string, std::string) {}) != kSuccess) {
        fail(agent, destroy, "firmware callback setup failed");
    }
    if (selected != "studio-serial-1" ||
        send_cloud(agent, "studio-serial-1", R"({"pushing":{"command":"pushall","sequence_id":"auxiliary-fence"}})", 0, 0) != kSuccess ||
        !capture.auxiliary_fence_new || capture.auxiliary_fence_old) {
        fail(agent, destroy, "printer recovery did not preserve the fresh auxiliary response");
    }
    if (connect_printer(agent, "studio-serial-1", "127.0.0.1", "user", "pass", false) != kSuccess) {
        fail(agent, destroy, "firmware local connection failed after printer recovery");
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

    const auto heartbeat_arm =
        std::filesystem::path(argv[2]) / "version-heartbeat-arm";
    std::filesystem::create_directory(heartbeat_arm);
    if (!wait_until(
            [&] { return capture.version_heartbeat_committed.load(); },
            Clock::now() + std::chrono::seconds(3)
        )) {
        fail(agent, destroy, "firmware version heartbeat synchronization failed");
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
        overlap_delay_ms < 1'000 || overlap_delay_ms >= 2'000) {
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

    if (send_local(
            agent,
            "studio-serial-1",
            command("upgrade_confirm", "l-generation-fence"),
            0,
            0
        ) != kSuccess ||
        disconnect_printer(agent) != kSuccess ||
        connect_printer(
            agent,
            "studio-serial-1",
            "127.0.0.1",
            "user",
            "pass",
            false
        ) != kSuccess) {
        fail(agent, destroy, "local firmware generation fence setup failed");
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(2'100));
    if (capture.stale_local_generation_callbacks != 0) {
        fail(agent, destroy, "stale local firmware acknowledgement reached the new connection");
    }

    Clock::time_point returned_at;
    std::atomic<bool> send_returned{false};
    int delayed_rc = -1;
    std::thread delayed([&] {
        delayed_rc = send_cloud(agent, "studio-serial-1", command("mc_for_ams_firmware_upgrade", "c-delay-reject"), 0, 0);
        returned_at = Clock::now();
        send_returned = true;
    });
    std::this_thread::sleep_for(std::chrono::milliseconds(100));
    const auto unrelated_rc =
        send_cloud(agent, "studio-serial-1", R"({"system":{"command":"unrelated"}})", 0, 0);
    if (send_returned || capture.delayed_callbacks != 0 || unrelated_rc != kInvalidResult) {
        std::cerr << "originating_send_returned=" << send_returned.load()
                  << " delayed_callbacks=" << capture.delayed_callbacks.load()
                  << " unrelated_rc=" << unrelated_rc << '\n';
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
        if (logout(agent, false) == kSuccess) capture.synchronous_reentrant_done = true;
    };
    if (send_cloud(
            agent,
            "studio-serial-1",
            R"({"info":{"command":"get_version","sequence_id":"c-synchronous-reentrant"}})",
            0,
            0
        ) != kConnectFailed ||
        !capture.synchronous_reentrant_done.load() ||
        change_user(agent, profile) != kSuccess ||
        get_print_info(agent, &code, &body) != kSuccess ||
        !select_and_subscribe() ||
        get_selected_machine(agent) != "studio-serial-1") {
        fail(agent, destroy, "synchronous firmware callback reentrant logout deadlocked");
    }
    capture.status_logout = [&] { logout(agent, false); };
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
    const auto status_logout_entered_at = Clock::now();
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
    if (!wait_until(
            [&] { return version_fence_started.load(); },
            Clock::now() + std::chrono::seconds(1)
        )) {
        std::cerr << "firmware generation fence request did not start\n";
        std::_Exit(2);
    }
    if (!wait_until(
            [&] { return capture.status_logout_completed.load(); },
            Clock::now() + std::chrono::seconds(8)
        )) {
        const auto elapsed_ms = std::chrono::duration_cast<std::chrono::milliseconds>(
            Clock::now() - status_logout_entered_at
        ).count();
        std::cerr << "status callback logout exceeded watchdog elapsed_ms="
                  << elapsed_ms << '\n';
        std::_Exit(2);
    }
    status_logout.join();
    version_fence.join();
    if (version_fence_rc != kConnectFailed || capture.forbidden_callbacks != 0 ||
        capture.firmware_status_callbacks_after_logout != 0 ||
        change_user(agent, profile) != kSuccess ||
        get_print_info(agent, &code, &body) != kSuccess ||
        !select_and_subscribe() ||
        get_selected_machine(agent) != "studio-serial-1") {
        fail(agent, destroy, "generation fence did not cancel synchronous firmware callback");
    }

    if (send_cloud(agent, "studio-serial-1", command("upgrade_confirm", "c-logout"), 0, 0) != kSuccess ||
        logout(agent, false) != kSuccess) fail(agent, destroy, "logout cancellation setup failed");
    std::this_thread::sleep_for(std::chrono::milliseconds(2'100));
    const bool logout_cancelled = capture.forbidden_callbacks == 0;
    if (!logout_cancelled || change_user(agent, profile) != kSuccess ||
        get_print_info(agent, &code, &body) != kSuccess ||
        !select_and_subscribe() ||
        get_selected_machine(agent) != "studio-serial-1") {
        fail(agent, destroy, "reentrant logout setup failed");
    }
    capture.reentrant_logout = [&] {
        if (logout(agent, false) == kSuccess) capture.reentrant_done = true;
    };
    if (send_cloud(agent, "studio-serial-1", command("upgrade_confirm", "c-reentrant"), 0, 0) != kSuccess ||
        !wait_until([&] { return capture.reentrant_done.load(); }, Clock::now() + std::chrono::milliseconds(2'100)) ||
        change_user(agent, profile) != kSuccess || get_print_info(agent, &code, &body) != kSuccess ||
        !select_and_subscribe() ||
        get_selected_machine(agent) != "studio-serial-1" ||
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
