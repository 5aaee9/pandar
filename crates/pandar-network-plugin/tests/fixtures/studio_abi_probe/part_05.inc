    if (printer_presence_mode) {
        const std::string profile = R"({"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"})";
        if (change_user(agent, profile) != 0) {
            fail(agent, destroy_agent, "printer presence profile setup failed");
        }
        TunnelCapture cloud;
        TunnelCapture local;
        std::atomic<int> printer_callbacks{0};
        std::atomic<int> ready_callbacks{0};
        std::atomic<int> disconnected_callbacks{0};
        std::atomic<int> local_connected_callbacks{0};
        std::atomic<int> local_lost_callbacks{0};
        if (set_message(agent, [&cloud](std::string dev_id, std::string body) {
                capture_tunnel_message(cloud, dev_id, body);
            }) != 0 ||
            set_local_message(agent, [&local](std::string dev_id, std::string body) {
                capture_tunnel_message(local, dev_id, body);
            }) != 0 ||
            set_printer_connected(agent, [&printer_callbacks](std::string) {
                ++printer_callbacks;
            }) != 0 ||
            set_local_connect(agent, [&local_connected_callbacks, &local_lost_callbacks](int state, std::string, std::string) {
                if (state == 0) ++local_connected_callbacks;
                if (state == 2) ++local_lost_callbacks;
            }) != 0 ||
            set_server_connected(agent, [&ready_callbacks, &disconnected_callbacks](int status, int reason) {
                if (status == 0 && reason == 0) ++ready_callbacks;
                else if (status < 0) ++disconnected_callbacks;
            }) != 0) {
            fail(agent, destroy_agent, "printer presence callback registration failed");
        }
        if (connect_server(agent) != 0 || ready_callbacks.load() != 1) {
            fail(agent, destroy_agent, "printer presence readiness setup failed");
        }
        add_subscribe(agent, {"studio-serial-1"});
        std::this_thread::sleep_for(std::chrono::milliseconds(150));
        if (printer_callbacks.load() != 0 || status_count(cloud, "studio-serial-1") != 0) {
            fail(agent, destroy_agent, "subscription emitted presence before a fresh observation");
        }
        if (!wait_until([&] {
                return hub_presence_step() >= 1 && printer_callbacks.load() == 1 &&
                    status_count(cloud, "studio-serial-1") == 1;
            })) {
            fail(agent, destroy_agent, "fresh online observation did not emit first heartbeat");
        }
        const std::string get_version =
            R"({"info":{"command":"get_version","sequence_id":"presence-version"}})";
        if (send_cloud(agent, "studio-serial-1", get_version, 0, 0) != 0 ||
            !wait_until([&] { return version_count(cloud, "studio-serial-1") == 1; }) ||
            connect_printer(agent, "studio-serial-1", "127.0.0.1", "user", "pass", false) != 0 ||
            local_connected_callbacks.load() != 1) {
            fail(agent, destroy_agent, "fresh online observation did not allow active tunnel setup");
        }
        if (!wait_until([&] { return hub_presence_step() >= 2; })) {
            fail(agent, destroy_agent, "offline observation was not requested");
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(150));
        if (printer_callbacks.load() != 1 || status_count(cloud, "studio-serial-1") != 1 ||
            disconnected_count(cloud, "studio-serial-1") != 1 ||
            disconnected_count(local, "studio-serial-1") != 1 || local_lost_callbacks.load() != 1) {
            fail(agent, destroy_agent, "typed offline transition emitted online presence");
        }
        const int cloud_versions_offline = version_count(cloud, "studio-serial-1");
        const int local_versions_offline = version_count(local, "studio-serial-1");
        send_cloud(agent, "studio-serial-1", get_version, 0, 0);
        send_printer(agent, "studio-serial-1", get_version, 0, 0);
        std::this_thread::sleep_for(std::chrono::milliseconds(150));
        if (version_count(cloud, "studio-serial-1") != cloud_versions_offline ||
            version_count(local, "studio-serial-1") != local_versions_offline) {
            fail(agent, destroy_agent, "offline get_version bypassed freshness eligibility");
        }
        if (!wait_until([&] { return hub_presence_step() >= 3; })) {
            fail(agent, destroy_agent, "timeout observation was not requested");
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(150));
        if (printer_callbacks.load() != 1 || status_count(cloud, "studio-serial-1") != 1 ||
            disconnected_callbacks.load() != 1 || is_server_connected(agent)) {
            fail(agent, destroy_agent, "timeout replayed stale presence or missed disconnect");
        }
        if (!wait_until([&] { return hub_presence_step() >= 4; })) {
            fail(agent, destroy_agent, "500 observation was not requested");
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(150));
        if (printer_callbacks.load() != 1 || status_count(cloud, "studio-serial-1") != 1 ||
            disconnected_count(cloud, "studio-serial-1") != 1 ||
            disconnected_count(local, "studio-serial-1") != 1 || is_server_connected(agent)) {
            fail(agent, destroy_agent, "500 replayed stale presence or recovered reachability");
        }
        if (!wait_until([&] {
                return hub_presence_step() >= 5 && printer_callbacks.load() == 2 &&
                    status_count(cloud, "studio-serial-1") == 2;
            }) || !is_server_connected(agent) || ready_callbacks.load() != 2) {
            fail(agent, destroy_agent, "fresh online recovery did not restore reachability and presence");
        }
        if (send_cloud(agent, "studio-serial-1", get_version, 0, 0) != 0 ||
            !wait_until([&] { return version_count(cloud, "studio-serial-1") == 2; }) ||
            !wait_until([&] { return hub_presence_step() >= 6; })) {
            fail(agent, destroy_agent, "recovery get_version or following heartbeat was missing");
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(150));
        if (ready_callbacks.load() != 2 || disconnected_callbacks.load() != 1 ||
            printer_callbacks.load() != 2 || status_count(cloud, "studio-serial-1") != 3 ||
            disconnected_count(cloud, "studio-serial-1") != 1) {
            fail(agent, destroy_agent, "stable recovery repeated transition callbacks");
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }

    if (start(agent) != 0) fail(agent, destroy_agent, "agent start failed");
    out.host = get_host(agent);
    if (!contains(out.host, "http://127.0.0.1:")) {
        fail(agent, destroy_agent, "frontend host did not use local webserver");
    }

    unsigned int http_code = 0;
    std::string http_body;
    int token_rc = (stale_token_mode || token_refresh_mode)
        ? -1
        : get_token(agent, "probe-ticket", &http_code, &http_body);
    if (stale_token_mode || token_refresh_mode) {
        if (!is_user_login(agent) || !contains(build_login_cmd(agent), "stale-token")) {
            fail(agent, destroy_agent, "stale token mode did not restore persisted login");
        }
    } else if (failure_mode) {
        if (token_rc == 0 || http_code != 401 || !contains(http_body, "invalid_plugin_ticket")) {
            fail(agent, destroy_agent, "ticket failure did not map to invalid_plugin_ticket");
        }
        assert_redacted_stable_error(
            agent,
            destroy_agent,
            http_body,
            "invalid_plugin_ticket",
            {"secret", "raw-ticket-message", "\"ticket\"", "\"token\"", "\"path\"", "/tmp/secret.3mf"}
        );
        std::cerr << "invalid_plugin_ticket\n";
        const std::string synthetic_profile = R"({"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant","tenant_name":"Tenant"})";
        if (change_user(agent, synthetic_profile) != 0) fail(agent, destroy_agent, "change_user failed in failure mode");
    } else {
        if (token_rc != 0 || http_code != 200) fail(agent, destroy_agent, "ticket exchange failed");
        if (!contains(http_body, "accessToken") || !contains(http_body, "probe-token")) {
            fail(agent, destroy_agent, "ticket exchange did not return Studio token fields");
        }
        std::string profile_body;
        int profile_rc = get_profile(agent, "probe-token", &http_code, &profile_body);
        if (profile_rc != 0 || http_code != 200) fail(agent, destroy_agent, "profile retrieval failed");
        if (!contains(profile_body, "uidStr") || !contains(profile_body, "probe-user") || !contains(profile_body, "Probe User")) {
            fail(agent, destroy_agent, "profile retrieval did not return stored profile content");
        }
        if (change_user(agent, profile_body) != 0) fail(agent, destroy_agent, "change_user failed");
        if (!is_user_login(agent)) fail(agent, destroy_agent, "expected user login before hub switch");
        switch_local_hub_target(out.host, "https://switched-hub.example.test");
        if (is_user_login(agent)) fail(agent, destroy_agent, "hub switch did not clear login state");
        if (contains(build_login_info(agent), "probe-token") || contains(build_login_cmd(agent), "probe-token")) {
            fail(agent, destroy_agent, "hub switch left old token in login envelope");
        }
        const char* original_hub = std::getenv("PANDAR_PLUGIN_HUB_URL");
        if (!original_hub || original_hub[0] == '\0') {
            fail(agent, destroy_agent, "missing original hub URL env");
        }
        switch_local_hub_target(out.host, original_hub);
        if (get_token(agent, "probe-ticket", &http_code, &http_body) != 0 || http_code != 200) {
            fail(agent, destroy_agent, "ticket exchange after hub switch recovery failed");
        }
        std::string recovered_profile;
        if (get_profile(agent, "probe-token", &http_code, &recovered_profile) != 0 || http_code != 200) {
            fail(agent, destroy_agent, "profile retrieval after hub switch recovery failed");
        }
        if (change_user(agent, recovered_profile) != 0) fail(agent, destroy_agent, "change_user after hub switch recovery failed");
    }

    out.login_command = build_login_cmd(agent);
    out.login_info = build_login_info(agent);
    if (!contains(out.login_command, "studio_userlogin") || !contains(out.login_info, "studio_userlogin")) {
        fail(agent, destroy_agent, "login envelopes lacked studio_userlogin");
    }

    std::string printers_body;
    out.printer_rc = get_print_info(agent, &http_code, &printers_body);
    if (failure_mode) {
        if (out.printer_rc == 0 || http_code != 401 || !contains(printers_body, "invalid_auth_token")) {
            fail(agent, destroy_agent, "printer failure did not map to invalid_auth_token");
        }
        assert_redacted_stable_error(
            agent,
            destroy_agent,
            printers_body,
            "invalid_auth_token",
            {"secret", "raw-auth-message", "\"ticket\"", "\"token\"", "\"path\"", "/tmp/secret.3mf"}
        );
        std::cerr << "invalid_auth_token\n";
        unsigned int recovery_code = 0;
        std::string recovery_body;
        if (get_print_info(agent, &recovery_code, &recovery_body) != 0 || recovery_code != 200) {
            fail(agent, destroy_agent, "failure-mode printer authorization recovery failed");
        }
    } else if (out.printer_rc != 0 || http_code != 200) {
        fail(agent, destroy_agent, "printer listing failed");
    }
    if (!get_selected_machine(agent).empty() ||
        set_selected_machine(agent, "studio-serial-1") != 0) {
        fail(agent, destroy_agent, "selected machine getter performed implicit session work");
    }

    if (token_rotation_offline_mode) {
        TunnelCapture cloud;
        TunnelCapture local;
        std::atomic<int> auth_rejected_callbacks{0};
        std::atomic<int> local_connected_callbacks{0};
        std::atomic<int> local_lost_callbacks{0};
        std::atomic<int> local_callback_clear_rc{kBambuInvalidResult};
        if (set_server_connected(agent, [&auth_rejected_callbacks](int status, int reason) {
                if (status == 5 && reason == 0) ++auth_rejected_callbacks;
            }) != 0 ||
            set_message(agent, [&cloud](std::string dev_id, std::string body) {
                capture_tunnel_message(cloud, dev_id, body);
            }) != 0 ||
            set_local_message(agent, [&](std::string dev_id, std::string body) {
                capture_tunnel_message(local, dev_id, body);
                if (body == R"({"event":{"event":"client.disconnected"}})") {
                    local_callback_clear_rc.store(set_local_connect(agent, {}));
                }
            }) != 0 ||
            set_local_connect(agent, [&local_connected_callbacks, &local_lost_callbacks](int status, std::string, std::string) {
                if (status == 0) ++local_connected_callbacks;
                if (status == 2) ++local_lost_callbacks;
            }) != 0 ||
            add_subscribe(agent, {"studio-serial-1"}) != 0 ||
            connect_printer(agent, "studio-serial-1", "127.0.0.1", "user", "pass", false) != 0) {
            fail(agent, destroy_agent, "offline token rotation active printer setup failed");
        }
        const int cloud_status_before = status_count(cloud, "studio-serial-1");
        const int local_status_before = status_count(local, "studio-serial-1");
        const std::string pushall =
            R"({"pushing":{"command":"pushall","sequence_id":"rotation-offline","version":1,"push_target":1}})";
        if (send_cloud(agent, "studio-serial-1", pushall, 0, 0) != kBambuConnectFailed ||
            !wait_until([&] {
                return disconnected_count(cloud, "studio-serial-1") == 1 &&
                    disconnected_count(local, "studio-serial-1") == 1 &&
                    local_callback_clear_rc.load() == 0;
            }) ||
            !contains(build_login_info(agent), "rotated-token") ||
            auth_rejected_callbacks.load() != 0 || local_connected_callbacks.load() != 1 ||
            local_lost_callbacks.load() != 0 ||
            status_count(cloud, "studio-serial-1") != cloud_status_before ||
            status_count(local, "studio-serial-1") != local_status_before ||
            send_printer(agent, "studio-serial-1", "M999", 0, 0) != kBambuConnectFailed) {
            fail(agent, destroy_agent, "offline token retry leaked stale presence or local send");
        }
        if (send_cloud(agent, "studio-serial-1", pushall, 0, 0) != kBambuConnectFailed ||
            disconnected_count(cloud, "studio-serial-1") != 1 ||
            disconnected_count(local, "studio-serial-1") != 1 ||
            local_lost_callbacks.load() != 0) {
            fail(agent, destroy_agent, "offline token retry repeated disconnect delivery");
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }

    if (token_rotation_mode) {
        TunnelCapture cloud;
        TunnelCapture local;
        std::atomic<int> auth_rejected_callbacks{0};
        std::atomic<int> printer_callbacks{0};
        std::atomic<bool> delete_first_subscription{true};
        std::atomic<int> local_connected_callbacks{0};
        std::atomic<int> local_lost_callbacks{0};
        if (set_server_connected(agent, [&auth_rejected_callbacks](int status, int reason) {
                if (status == 5 && reason == 0) ++auth_rejected_callbacks;
            }) != 0 ||
            set_message(agent, [&cloud](std::string dev_id, std::string body) {
                capture_tunnel_message(cloud, dev_id, body);
            }) != 0 ||
            set_local_message(agent, [&local](std::string dev_id, std::string body) {
                capture_tunnel_message(local, dev_id, body);
            }) != 0 ||
            set_printer_connected(agent, [&](std::string) {
                ++printer_callbacks;
                if (delete_first_subscription.exchange(false)) {
                    del_subscribe(agent, {"studio-serial-1"});
                }
            }) != 0 ||
            set_local_connect(agent, [&local_connected_callbacks, &local_lost_callbacks](int status, std::string, std::string) {
                if (status == 0) ++local_connected_callbacks;
                if (status == 2) ++local_lost_callbacks;
            }) != 0 ||
            add_subscribe(agent, {"studio-serial-1"}) != 0) {
            fail(agent, destroy_agent, "token rotation active printer setup failed");
        }
        if (printer_callbacks.load() != 1 || status_count(cloud, "studio-serial-1") != 1 ||
            add_subscribe(agent, {"studio-serial-1"}) != 0 ||
            status_count(cloud, "studio-serial-1") != 2 || printer_callbacks.load() != 1 ||
            connect_printer(agent, "studio-serial-1", "127.0.0.1", "user", "pass", false) != 0) {
            fail(agent, destroy_agent, "selected printer lost status targeting after reentrant unsubscribe");
        }
        const int status_before_rotation = status_count(cloud, "studio-serial-1");
        const std::string pushall =
            R"({"pushing":{"command":"pushall","sequence_id":"rotation","version":1,"push_target":1}})";
        if (send_cloud(agent, "studio-serial-1", pushall, 0, 0) != 0 ||
            !wait_until([&] {
                return status_count(cloud, "studio-serial-1") == status_before_rotation + 1;
            }) ||
            !contains(build_login_info(agent), "rotated-token") ||
            get_selected_machine(agent) != "studio-serial-1" ||
            auth_rejected_callbacks.load() != 0 || local_lost_callbacks.load() != 0 ||
            local_connected_callbacks.load() != 1 || printer_callbacks.load() != 1) {
            fail(agent, destroy_agent, "successful token rotation changed active printer state or emitted code 5");
        }
        const std::string get_version =
            R"({"info":{"command":"get_version","sequence_id":"rotation-version"}})";
        if (send_printer(agent, "studio-serial-1", get_version, 0, 0) != 0 ||
            !wait_until([&] { return version_count(local, "studio-serial-1") == 1; })) {
            fail(agent, destroy_agent, "successful token rotation lost the active local generation");
        }
        const int status_before_invalid = status_count(cloud, "studio-serial-1");
        if (send_cloud(agent, "studio-serial-1", pushall, 0, 0) != kBambuConnectFailed ||
            !wait_until([&] {
                return disconnected_count(cloud, "studio-serial-1") == 1 &&
                    disconnected_count(local, "studio-serial-1") == 1 &&
                    local_lost_callbacks.load() == 1;
            }) ||
            status_count(cloud, "studio-serial-1") != status_before_invalid ||
            auth_rejected_callbacks.load() != 0 || !is_server_connected(agent)) {
            fail(agent, destroy_agent, "invalid rotated status replayed stale presence or changed Hub reachability");
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }
