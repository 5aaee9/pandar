    if (no_auth_recovery_mode) {
        std::atomic<int> login_callbacks{0};
        std::atomic<int> server_callbacks{0};
        std::atomic<int> http_error_callbacks{0};
        if (set_user_login(agent, [&login_callbacks](int online, bool login) {
                if (online == 1 && login) ++login_callbacks;
            }) != 0 ||
            set_server_connected(agent, [&server_callbacks](int status, int reason) {
                if (status == 0 && reason == 0) ++server_callbacks;
            }) != 0 ||
            set_http_error(agent, [&http_error_callbacks](unsigned, std::string) {
                ++http_error_callbacks;
            }) != 0) {
            fail(agent, destroy_agent, "no-auth recovery callback registration failed");
        }
        if (start(agent) != 0 || is_user_login(agent) || http_error_callbacks.load() != 1) {
            fail(agent, destroy_agent, "initial no-auth connect failure did not stay logged out");
        }
        std::ofstream(
            std::filesystem::path(config_dir) / "no-auth-initial-failure-observed"
        ) << "observed\n";

        const auto recovered = [&] {
            return is_user_login(agent) &&
                get_user_id(agent) == "recovered-user" &&
                login_callbacks.load() == 1 && server_callbacks.load() == 1;
        };
        const auto recovery_deadline =
            std::chrono::steady_clock::now() + std::chrono::seconds(6);
        while (!recovered() && std::chrono::steady_clock::now() < recovery_deadline) {
            std::this_thread::sleep_for(std::chrono::milliseconds(25));
        }
        if (!recovered() ||
            !contains(build_login_cmd(agent), "recovered-token") ||
            login_callbacks.load() != 1 || server_callbacks.load() != 1) {
            std::ifstream trace_file(
                std::filesystem::path(config_dir) / "pandar-network-plugin.trace.log"
            );
            std::stringstream trace;
            trace << trace_file.rdbuf();
            fail(
                agent, destroy_agent,
                "no-auth session did not recover after Hub startup; trace=" + trace.str()
            );
        }

        unsigned int profile_code = 0;
        std::string profile;
        if (get_profile(agent, "", &profile_code, &profile) != 0 || profile_code != 200 ||
            !contains(profile, "Recovered User")) {
            fail(agent, destroy_agent, "recovered no-auth profile was unavailable");
        }

        std::this_thread::sleep_for(std::chrono::milliseconds(4500));
        if (login_callbacks.load() != 1 || server_callbacks.load() != 1 ||
            http_error_callbacks.load() != 1 ||
            connect_server(agent) != 0 || server_callbacks.load() != 1) {
            fail(agent, destroy_agent, "stable no-auth recovery repeated callbacks");
        }

        std::ifstream counts_file(
            std::filesystem::path(config_dir) / "no-auth-recovery-counts"
        );
        std::string counts;
        std::getline(counts_file, counts);
        if (counts != "posts=1 tokens=1 audits=1 printers=1") {
            fail(agent, destroy_agent, "no-auth recovery created duplicate server state: " + counts);
        }

        destroy_agent(agent);
        print_json(out);
        return 0;
    }

    if (official_no_auth_lifecycle_mode) {
        std::atomic<int> false_login_callbacks{0};
        init_log(agent);
        set_cert_file(agent, "probe-cert", "slicer_base64.cer");
        set_extra_http_header(agent, {{"X-Official-Lifecycle-Probe", "1"}});

        if (official_no_auth_logout_recovery_mode &&
            set_user_login(agent, [&false_login_callbacks](int, bool login) {
                if (!login) ++false_login_callbacks;
            }) != 0) {
            fail(agent, destroy_agent, "logout recovery callback registration failed");
        }

        if (set_server_error(agent, [](std::string, int) {}) != 0 ||
            set_server_connected(agent, [](int, int) {}) != 0 ||
            set_printer_connected(agent, [](std::string) {}) != 0 ||
            set_country_callback(agent, [] { return std::string("US"); }) != 0 ||
            set_subscribe_failure(agent, [](std::string) {}) != 0 ||
            set_local_connect(agent, [](int, std::string, std::string) {}) != 0 ||
            set_message(agent, [](std::string, std::string) {}) != 0 ||
            set_user_message(agent, [](std::string, std::string) {}) != 0 ||
            set_local_message(agent, [](std::string, std::string) {}) != 0 ||
            set_queue_on_main(agent, [](std::function<void()>) {}) != 0 ||
            set_country_code(agent, "US") != 0) {
            fail(agent, destroy_agent, "official Studio callback registration failed");
        }

        if (start(agent) != 0) {
            fail(agent, destroy_agent, "official Studio no-auth start failed");
        }
        if (!get_selected_machine(agent).empty()) {
            fail(agent, destroy_agent, "official Studio lifecycle restored an unexpected machine");
        }

        std::ifstream initial_trace_file(
            std::filesystem::path(config_dir) / "pandar-network-plugin.trace.log"
        );
        std::stringstream initial_trace;
        initial_trace << initial_trace_file.rdbuf();
        if (!contains(initial_trace.str(), "no-auth response status=2 http_code=0")) {
            fail(
                agent,
                destroy_agent,
                "official Studio lifecycle did not observe the initial no-auth failure; trace=" +
                    initial_trace.str()
            );
        }
        if (official_no_auth_logout_recovery_mode) {
            std::this_thread::sleep_for(std::chrono::milliseconds(2100));
            if (user_logout(agent, false) != 0 || user_logout(agent, false) != 0) {
                fail(agent, destroy_agent, "logged-out notification was not idempotent");
            }
        }
        std::ofstream(
            std::filesystem::path(config_dir) / "no-auth-initial-failure-observed"
        ) << "observed\n";

        const auto login_file =
            std::filesystem::path(config_dir) / "pandar-plugin-login.json";
        const auto counts_file =
            std::filesystem::path(config_dir) / "no-auth-recovery-counts";
        const auto deadline =
            std::chrono::steady_clock::now() + std::chrono::seconds(10);
        while ((!std::filesystem::exists(login_file) ||
                !std::filesystem::exists(counts_file)) &&
               std::chrono::steady_clock::now() < deadline) {
            std::this_thread::sleep_for(std::chrono::milliseconds(25));
        }

        std::ifstream login_stream(login_file);
        std::stringstream login;
        login << login_stream.rdbuf();
        std::ifstream counts_stream(counts_file);
        std::string counts;
        std::getline(counts_stream, counts);
        if (!contains(login.str(), "recovered-token") ||
            false_login_callbacks.load() != 0 ||
            counts != "posts=1 tokens=1 audits=1 printers=1") {
            std::ifstream trace_file(
                std::filesystem::path(config_dir) / "pandar-network-plugin.trace.log"
            );
            std::stringstream trace;
            trace << trace_file.rdbuf();
            fail(
                agent,
                destroy_agent,
                "official Studio zero-touch no-auth recovery failed; counts=" + counts +
                    "; false_login_callbacks=" +
                    std::to_string(false_login_callbacks.load()) +
                    "; trace=" + trace.str()
            );
        }

        destroy_agent(agent);
        print_json(out);
        return 0;
    }
