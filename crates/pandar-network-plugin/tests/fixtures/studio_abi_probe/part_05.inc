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
            }) != 0 ||
            connect_server(agent) != 0 ||
            !wait_until([&] { return ready_callbacks.load() == 1; }) ||
            add_subscribe(agent, {"studio-serial-1"}) != 0) {
            fail(agent, destroy_agent, "printer presence readiness setup failed");
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(150));
        if (printer_callbacks.load() != 0 || status_count(cloud, "studio-serial-1") != 0) {
            fail(agent, destroy_agent, "empty snapshot fabricated printer presence");
        }

        const auto directory = std::filesystem::path(config_dir);
        std::filesystem::create_directory(directory / "presence-online");
        if (!wait_until([&] {
                return hub_presence_step() >= 1 && printer_callbacks.load() == 1 &&
                    status_count(cloud, "studio-serial-1") == 1;
            })) {
            fail(agent, destroy_agent, "streamed addition did not emit online presence");
        }
        const std::string get_version =
            R"({"info":{"command":"get_version","sequence_id":"presence-version"}})";
        if (send_cloud(agent, "studio-serial-1", get_version, 0, 0) != 0 ||
            !wait_until([&] { return version_count(cloud, "studio-serial-1") == 1; }) ||
            connect_printer(agent, "studio-serial-1", "127.0.0.1", "user", "pass", false) != 0 ||
            local_connected_callbacks.load() != 1) {
            fail(agent, destroy_agent, "fresh stream record did not allow active tunnels");
        }

        std::filesystem::create_directory(directory / "presence-offline");
        if (!wait_until([&] { return hub_presence_step() >= 2; })) {
            fail(agent, destroy_agent, "offline stream update was not served");
        }
        if (!wait_until([&] {
                return disconnected_count(cloud, "studio-serial-1") == 1 &&
                    disconnected_count(local, "studio-serial-1") == 1 &&
                    local_lost_callbacks.load() == 1;
            }) || printer_callbacks.load() != 1) {
            fail(agent, destroy_agent, "offline stream update ordering changed");
        }

        std::filesystem::create_directory(directory / "presence-reconnect");
        if (!wait_until([&] {
                return hub_presence_step() >= 3 && printer_callbacks.load() == 2 &&
                    status_count(cloud, "studio-serial-1") == 2;
            }) || disconnected_callbacks.load() != 0 || ready_callbacks.load() != 1 ||
            !is_server_connected(agent)) {
            fail(agent, destroy_agent, "short stream recovery replayed or disconnected");
        }

        std::filesystem::create_directory(directory / "presence-remove");
        if (!wait_until([&] {
                return hub_presence_step() >= 4 &&
                    disconnected_count(cloud, "studio-serial-1") == 2;
            })) {
            fail(agent, destroy_agent, "streamed removal did not deliver offline once");
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(250));
        if (disconnected_count(cloud, "studio-serial-1") != 2 ||
            status_count(cloud, "studio-serial-1") != 2 ||
            disconnected_callbacks.load() != 0) {
            fail(agent, destroy_agent, "stream removal or recovery callbacks repeated");
        }
        std::filesystem::create_directory(directory / "presence-done");
        hub_presence_step();
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


    std::atomic<int> stale_code5{0};
    if (token_refresh_mode) {
        // Stream era: the stale-token stream dial is refused and surfaces
        // reason code 5 exactly once; rotate explicitly from there.
        if (set_server_connected(agent, [&stale_code5](int status, int reason) {
                if (status == 5 && reason == 0) ++stale_code5;
            }) != 0) {
            fail(agent, destroy_agent, "stale rejection callback setup failed");
        }
        if (!wait_until([&] { return stale_code5.load() == 1; })) {
            fail(agent, destroy_agent, "stale stream rejection did not surface code 5");
        }
        const std::string probe_profile =
            R"({"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"})";
        if (change_user(agent, probe_profile) != 0) {
            fail(agent, destroy_agent, "probe-token rotation failed");
        }
    }
    std::string printers_body;
    out.printer_rc = get_print_info(agent, &http_code, &printers_body);
    if (failure_mode) {
        if (out.printer_rc == 0 || http_code != 503 ||
            !contains(printers_body, "cache_initializing")) {
            fail(agent, destroy_agent, "printer failure did not map to a redacted cache error");
        }
        assert_redacted_stable_error(
            agent,
            destroy_agent,
            printers_body,
            "cache_initializing",
            {"secret", "raw-auth-message", "\"ticket\"", "\"token\"", "\"path\"", "/tmp/secret.3mf"}
        );
        const std::string recovery_profile = R"({"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant","tenant_name":"Tenant"})";
        if (change_user(agent, recovery_profile) != 0) {
            fail(agent, destroy_agent, "failure-mode credential recovery was not accepted");
        }
        unsigned int recovery_code = 0;
        std::cerr << "invalid_auth_token\n";
        std::string recovery_body;
        bool recovered = false;
        const auto recovery_deadline =
            std::chrono::steady_clock::now() + std::chrono::seconds(10);
        while (std::chrono::steady_clock::now() < recovery_deadline) {
            if (get_print_info(agent, &recovery_code, &recovery_body) == 0 &&
                recovery_code == 200) {
                recovered = true;
                break;
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(100));
        }
        if (!recovered) {
            fail(agent, destroy_agent, "failure-mode printer authorization recovery failed");
        }
    } else if (out.printer_rc != 0 || http_code != 200) {
        fail(agent, destroy_agent, "printer listing failed");
    }
    if (!get_selected_machine(agent).empty() ||
        set_selected_machine(agent, "studio-serial-1") != 0) {
        fail(agent, destroy_agent, "selected machine getter performed implicit session work");
    }
