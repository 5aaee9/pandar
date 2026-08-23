    if (connection_readiness_mode) {
        const std::string profile = R"({"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"})";
        if (change_user(agent, profile) != 0) {
            fail(agent, destroy_agent, "connection readiness profile setup failed");
        }
        std::atomic<int> callbacks{0};
        std::atomic<int> successful_callbacks{0};
        if (set_server_connected(agent, [&callbacks, &successful_callbacks](int status, int reason) {
                if (reason != 0) return;
                ++callbacks;
                if (status == 0) ++successful_callbacks;
            }) != 0) {
            fail(agent, destroy_agent, "connection readiness callback registration failed");
        }
        if (is_server_connected(agent)) {
            fail(agent, destroy_agent, "configured Hub URL was treated as proven reachable");
        }
        if (connect_server(agent) != 0 || refresh_connection(agent) != 0) {
            fail(agent, destroy_agent, "lifecycle connect requests were not accepted");
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(300));
        if (is_server_connected(agent) || callbacks.load() != 0) {
            fail(agent, destroy_agent, "open stream without a snapshot was treated as connected");
        }
        unsigned int initializing_code = 0;
        std::string initializing_body;
        const auto initializing_started = std::chrono::steady_clock::now();
        const int initializing_rc =
            get_print_info(agent, &initializing_code, &initializing_body);
        const auto initializing_elapsed = std::chrono::steady_clock::now() - initializing_started;
        if (initializing_rc == 0 || initializing_code != 503 ||
            initializing_body != R"({"error":"cache_initializing"})" ||
            initializing_elapsed < std::chrono::seconds(29) ||
            initializing_elapsed > std::chrono::seconds(35)) {
            fail(agent, destroy_agent, "cache initialization timeout contract changed");
        }
        std::filesystem::create_directory(
            std::filesystem::path(config_dir) / "stream-release-snapshot"
        );
        unsigned int printers_code = 0;
        std::string printers_body;
        if (get_print_info(agent, &printers_code, &printers_body) != 0 ||
            printers_code != 200 || !contains(printers_body, "studio-serial-1")) {
            fail(agent, destroy_agent, "released snapshot did not serve the first device list");
        }
        if (!wait_until([&] {
                return is_server_connected(agent) && callbacks.load() == 1 &&
                    successful_callbacks.load() == 1;
            })) {
            fail(agent, destroy_agent, "snapshot commit did not mark the server connected");
        }
        if (refresh_connection(agent) != 0 || !is_server_connected(agent) ||
            callbacks.load() != 1) {
            fail(agent, destroy_agent, "unchanged ready stream repeated the callback");
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }

    if (background_timeout_mode) {
        const std::string profile = R"({"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"})";
        if (change_user(agent, profile) != 0) {
            fail(agent, destroy_agent, "background timeout profile setup failed");
        }
        TunnelCapture cloud;
        std::atomic<int> connected_callbacks{0};
        std::atomic<int> disconnected_callbacks{0};
        if (set_server_connected(agent, [&connected_callbacks, &disconnected_callbacks](int status, int reason) {
                if (reason != 0) return;
                if (status == 0) ++connected_callbacks;
                else ++disconnected_callbacks;
            }) != 0 ||
            set_message(agent, [&cloud](std::string dev_id, std::string body) {
                capture_tunnel_message(cloud, dev_id, body);
            }) != 0) {
            fail(agent, destroy_agent, "background timeout callback registration failed");
        }
        std::string printers_body;
        unsigned int printers_code = 0;
        if (get_print_info(agent, &printers_code, &printers_body) != 0 || printers_code != 200) {
            fail(agent, destroy_agent, "background timeout printer seed failed");
        }
        add_subscribe(agent, {"studio-serial-1"});

        // Transient stream close: the worker must recover silently, keep the
        // cached devices, and never emit -2 or poll the retired printer list.
        std::filesystem::create_directory(
            std::filesystem::path(config_dir) / "stream-drop-now"
        );
        if (!wait_until([&] {
                return std::filesystem::exists(
                    std::filesystem::path(config_dir) / "stream-redial-served"
                );
            })) {
            fail(agent, destroy_agent, "stream did not redial after a transient close");
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(400));
        if (disconnected_callbacks.load() != 0 || !is_server_connected(agent)) {
            fail(agent, destroy_agent, "transient stream close emitted a disconnect");
        }
        unsigned int retained_code = 0;
        std::string retained_body;
        if (get_print_info(agent, &retained_code, &retained_body) != 0 ||
            retained_code != 200 || !contains(retained_body, "studio-serial-1")) {
            fail(agent, destroy_agent, "cache was not retained across a transient close");
        }
        // Full outage: the hub goes dark; the bounded healthz observation
        // must disconnect the shim exactly once.
        std::filesystem::create_directory(
            std::filesystem::path(config_dir) / "stream-go-dark"
        );
        const auto disconnect_deadline =
            std::chrono::steady_clock::now() + std::chrono::seconds(45);
        while (disconnected_callbacks.load() < 1 &&
               std::chrono::steady_clock::now() < disconnect_deadline) {
            std::this_thread::sleep_for(std::chrono::milliseconds(50));
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(400));
        if (disconnected_callbacks.load() != 1 || connected_callbacks.load() < 1) {
            fail(
                agent,
                destroy_agent,
                "healthz failure did not disconnect exactly once; disconnected=" +
                    std::to_string(disconnected_callbacks.load()) + " connected=" +
                    std::to_string(connected_callbacks.load())
            );
        }
        unsigned int outage_code = 0;
        std::string outage_body;
        if (get_print_info(agent, &outage_code, &outage_body) != 0 || outage_code != 200) {
            fail(agent, destroy_agent, "outage discarded the last known printer cache");
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }

    if (stream_unavailable_mode) {
        const std::string profile = R"({"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"})";
        if (change_user(agent, profile) != 0) {
            fail(agent, destroy_agent, "stream unavailable profile setup failed");
        }
        TunnelCapture cloud;
        std::atomic<int> http_errors{0};
        std::atomic<int> disconnected_callbacks{0};
        std::atomic<unsigned> error_code{0};
        std::mutex error_mutex;
        std::string error_body;
        if (set_message(agent, [&cloud](std::string dev_id, std::string body) {
                capture_tunnel_message(cloud, dev_id, body);
            }) != 0 ||
            set_http_error(agent, [&](unsigned code, std::string body) {
                error_code.store(code);
                {
                    std::lock_guard<std::mutex> lock(error_mutex);
                    error_body = std::move(body);
                }
                ++http_errors;
            }) != 0 ||
            set_server_connected(agent, [&disconnected_callbacks](int status, int reason) {
                if (status < 0 && reason == 0) ++disconnected_callbacks;
            }) != 0) {
            fail(agent, destroy_agent, "stream unavailable callback registration failed");
        }
        unsigned int printers_code = 0;
        std::string printers_body;
        if (get_print_info(agent, &printers_code, &printers_body) != 0 ||
            printers_code != 200 || add_subscribe(agent, {"studio-serial-1"}) != 0) {
            fail(agent, destroy_agent, "stream unavailable cache seed failed");
        }
        const auto directory = std::filesystem::path(config_dir);
        std::filesystem::create_directory(directory / "stream-go-dark");
        const auto diagnosis_deadline =
            std::chrono::steady_clock::now() + std::chrono::seconds(45);
        while (http_errors.load() < 1 &&
               std::chrono::steady_clock::now() < diagnosis_deadline) {
            std::this_thread::sleep_for(std::chrono::milliseconds(50));
        }
        std::string observed_error;
        {
            std::lock_guard<std::mutex> lock(error_mutex);
            observed_error = error_body;
        }
        if (http_errors.load() != 1 || error_code.load() != 503 ||
            observed_error != R"({"error":"printer_event_stream_unavailable"})" ||
            disconnected_callbacks.load() != 0 || !is_server_connected(agent) ||
            disconnected_count(cloud, "studio-serial-1") != 1) {
            fail(agent, destroy_agent, "healthy Hub stream diagnosis was not exact");
        }
        unsigned int stale_code = 0;
        std::string stale_body;
        if (get_print_info(agent, &stale_code, &stale_body) != 0 || stale_code != 200 ||
            !contains(stale_body, R"("dev_online":false)") ||
            !contains(stale_body, R"("online":false)")) {
            fail(agent, destroy_agent, "stale cache did not project devices offline");
        }
        std::filesystem::create_directory(directory / "stream-healthy-recover");
        const auto recovery_deadline =
            std::chrono::steady_clock::now() + std::chrono::seconds(35);
        while ((!std::filesystem::exists(directory / "stream-healthy-recovery-served") ||
                status_count(cloud, "studio-serial-1") < 2) &&
               std::chrono::steady_clock::now() < recovery_deadline) {
            std::this_thread::sleep_for(std::chrono::milliseconds(50));
        }
        if (!std::filesystem::exists(directory / "stream-healthy-recovery-served") ||
            status_count(cloud, "studio-serial-1") < 2 || http_errors.load() != 1 ||
            disconnected_callbacks.load() != 0 || !is_server_connected(agent)) {
            fail(agent, destroy_agent, "stream recovery did not restore exactly once");
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }

    if (freshness_claim_mode) {
        const std::string profile = R"({"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"})";
        if (change_user(agent, profile) != 0) {
            fail(agent, destroy_agent, "freshness claim profile setup failed");
        }
        std::string printers_body;
        unsigned int printers_code = 0;
        if (get_print_info(agent, &printers_code, &printers_body) != 0 || printers_code != 200) {
            fail(agent, destroy_agent, "freshness claim printer seed failed");
        }
        if (connect_printer(agent, "studio-serial-1", "127.0.0.1", "user", "pass", false) !=
                kBambuConnectFailed ||
            send_printer(agent, "studio-serial-1", "M999", 0, 0) != kBambuConnectFailed) {
            fail(agent, destroy_agent, "unclaimed local connection was not rolled back");
        }
        TunnelCapture cloud;
        TunnelCapture local;
        std::atomic<int> connected_callbacks{0};
        std::atomic<int> local_connected_callbacks{0};
        if (set_message(agent, [&cloud](std::string dev_id, std::string body) {
                capture_tunnel_message(cloud, dev_id, body);
            }) != 0 ||
            set_local_message(agent, [&local](std::string dev_id, std::string body) {
                capture_tunnel_message(local, dev_id, body);
            }) != 0 ||
            set_printer_connected(agent, [&connected_callbacks](std::string) {
                ++connected_callbacks;
            }) != 0 ||
            set_local_connect(agent, [&local_connected_callbacks](int status, std::string, std::string) {
                if (status == 0) ++local_connected_callbacks;
            }) != 0 ||
            add_subscribe(agent, {"studio-serial-1"}) != 0 ||
            connect_printer(agent, "studio-serial-1", "127.0.0.1", "user", "pass", false) != 0 ||
            !wait_until([&] {
                return connected_callbacks.load() >= 1 &&
                    local_connected_callbacks.load() == 1 &&
                    status_count(cloud, "studio-serial-1") >= 1;
            })) {
            fail(
                agent,
                destroy_agent,
                "freshness claim active subscription setup failed connected=" +
                    std::to_string(connected_callbacks.load()) + " local=" +
                    std::to_string(local_connected_callbacks.load()) + " status=" +
                    std::to_string(status_count(cloud, "studio-serial-1"))
            );
        }

        const int connected_before_update = connected_callbacks.load();
        int version_rc = -1;
        std::thread version_request([&] {
            version_rc = send_cloud(
                agent,
                "studio-serial-1",
                R"({"info":{"command":"get_version","sequence_id":"freshness-claim"}})",
                0,
                0
            );
        });
        const auto directory = std::filesystem::path(config_dir);
        const auto version_entered = directory / "freshness-version-entered";
        if (!wait_until([&] { return std::filesystem::exists(version_entered); })) {
            version_request.join();
            fail(agent, destroy_agent, "freshness claim version request did not enter Hub");
        }
        std::filesystem::create_directory(directory / "freshness-stream-update");
        version_request.join();
        if (version_rc != kBambuConnectFailed ||
            version_count(cloud, "studio-serial-1") != 0) {
            fail(agent, destroy_agent, "stale version response escaped its cache fence");
        }
        if (!wait_until([&] {
                return status_count(cloud, "studio-serial-1") >= 2 &&
                    contains(status_body(cloud, "studio-serial-1"), R"("mc_percent":73)");
            })) {
            fail(agent, destroy_agent, "stream update did not replace the cached status");
        }
        const int local_send_rc =
            send_printer(agent, "studio-serial-1", "M140 S60", 0, 0);
        const std::string pushall =
            R"({"pushing":{"command":"pushall","sequence_id":"freshness-retry","version":1,"push_target":1}})";
        if (local_send_rc != 0 ||
            send_cloud(agent, "studio-serial-1", pushall, 0, 0) != 0 ||
            connected_callbacks.load() != connected_before_update) {
            fail(agent, destroy_agent, "fresh stream state did not survive stale final claim");
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }
