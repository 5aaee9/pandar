    if (firmware_claim_race_mode) {
        const std::string profile = R"({"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"})";
        std::string printers_body;
        unsigned int printers_code = 0;
        std::atomic<int> stale_callbacks{0};
        if (change_user(agent, profile) != 0 ||
            get_print_info(agent, &printers_code, &printers_body) != 0 || printers_code != 200 ||
            set_message(agent, [&stale_callbacks](std::string, std::string body) {
                if (body.find("c-final-claim-race") != std::string::npos) ++stale_callbacks;
            }) != 0 ||
            add_subscribe(agent, {"studio-serial-1"}) != 0) {
            fail(agent, destroy_agent, "firmware final claim race setup failed");
        }

        const int firmware_rc = send_cloud(
                agent,
                "studio-serial-1",
                R"({"upgrade":{"command":"upgrade_confirm","sequence_id":"c-final-claim-race","src_id":1}})",
                0,
                0
            );
        if (firmware_rc != 0) {
            fail(agent, destroy_agent, "firmware final claim handoff failed");
        }

        int exchange_rc = -1;
        unsigned int exchange_code = 0;
        std::string exchange_body;
        std::thread token_exchange([&] {
            exchange_rc = get_token(
                agent,
                "final-claim-ticket",
                &exchange_code,
                &exchange_body
            );
        });
        const auto exchange_entered =
            std::filesystem::path(config_dir) / "firmware-claim-exchange-entered";
        if (!wait_until([&] { return std::filesystem::exists(exchange_entered); })) {
            token_exchange.join();
            fail(agent, destroy_agent, "firmware final claim token exchange did not enter Hub");
        }
        token_exchange.join();
        std::this_thread::sleep_for(std::chrono::milliseconds(300));
        if (firmware_rc != 0 || exchange_rc != 0 || exchange_code != 200 ||
            stale_callbacks.load() != 0) {
            fail(
                agent,
                destroy_agent,
                "rotated generation escaped firmware final claim firmware_rc=" +
                    std::to_string(firmware_rc) + " exchange_rc=" +
                    std::to_string(exchange_rc) + " exchange_code=" +
                    std::to_string(exchange_code) + " stale_callbacks=" +
                    std::to_string(stale_callbacks.load())
            );
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }

    if (callback_order_mode) {
        const std::string profile = R"({"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"})";
        std::string printers_body;
        unsigned int printers_code = 0;
        if (change_user(agent, profile) != 0 ||
            get_print_info(agent, &printers_code, &printers_body) != 0 ||
            printers_code != 200) {
            fail(agent, destroy_agent, "callback order printer seed failed");
        }

        TunnelCapture cloud;
        std::atomic<bool> block_seed_status{true};
        std::atomic<bool> seed_callback_entered{false};
        std::atomic<bool> release_callback_gate{false};
        std::atomic<int> connected_callbacks{0};
        std::atomic<int> invalid_connected_topics{0};
        if (set_message(agent, [&](std::string dev_id, std::string body) {
                if (contains(body, R"("command":"push_status")") &&
                    block_seed_status.exchange(false)) {
                    seed_callback_entered.store(true, std::memory_order_release);
                    while (!release_callback_gate.load(std::memory_order_acquire)) {
                        std::this_thread::sleep_for(std::chrono::milliseconds(5));
                    }
                }
                capture_tunnel_message(cloud, dev_id, body);
            }) != 0 ||
            set_printer_connected(agent, [&](std::string topic) {
                if (topic == "tunnel/studio-serial-1") {
                    ++connected_callbacks;
                } else {
                    ++invalid_connected_topics;
                }
            }) != 0) {
            fail(agent, destroy_agent, "callback order callback registration failed");
        }

        int subscribe_rc = -1;
        std::thread seed_subscription([&] {
            subscribe_rc = add_subscribe(agent, {"studio-serial-1"});
        });
        if (!wait_until([&] {
                return seed_callback_entered.load(std::memory_order_acquire);
            })) {
            release_callback_gate.store(true, std::memory_order_release);
            seed_subscription.join();
            fail(agent, destroy_agent, "seed status did not acquire the callback gate");
        }

        const std::string offline_pushall =
            R"({"pushing":{"command":"pushall","sequence_id":"callback-order-offline","version":1,"push_target":1}})";
        int offline_rc = -1;
        std::thread offline_refresh([&] {
            offline_rc = send_cloud(agent, "studio-serial-1", offline_pushall, 0, 0);
        });
        const auto offline_entered =
            std::filesystem::path(config_dir) / "callback-order-offline-entered";
        if (!wait_until([&] { return std::filesystem::exists(offline_entered); })) {
            release_callback_gate.store(true, std::memory_order_release);
            seed_subscription.join();
            offline_refresh.join();
            fail(agent, destroy_agent, "offline refresh did not enter the Hub");
        }
        std::filesystem::create_directory(
            std::filesystem::path(config_dir) / "callback-order-release-offline"
        );
        const auto offline_responded =
            std::filesystem::path(config_dir) / "callback-order-offline-responded";
        if (!wait_until([&] { return std::filesystem::exists(offline_responded); })) {
            release_callback_gate.store(true, std::memory_order_release);
            seed_subscription.join();
            offline_refresh.join();
            fail(agent, destroy_agent, "Hub did not return the offline refresh");
        }

        const std::string recovery_pushall =
            R"({"pushing":{"command":"pushall","sequence_id":"callback-order-recovery","version":1,"push_target":1}})";
        int recovery_rc = -1;
        std::thread recovery_refresh([&] {
            recovery_rc = send_cloud(agent, "studio-serial-1", recovery_pushall, 0, 0);
        });
        const auto recovery_entered =
            std::filesystem::path(config_dir) / "callback-order-recovery-entered";
        if (!wait_until([&] { return std::filesystem::exists(recovery_entered); })) {
            release_callback_gate.store(true, std::memory_order_release);
            seed_subscription.join();
            offline_refresh.join();
            recovery_refresh.join();
            fail(agent, destroy_agent, "recovery refresh did not enter the Hub");
        }
        std::filesystem::create_directory(
            std::filesystem::path(config_dir) / "callback-order-release-recovery"
        );
        const auto recovery_responded =
            std::filesystem::path(config_dir) / "callback-order-recovery-responded";
        if (!wait_until([&] { return std::filesystem::exists(recovery_responded); })) {
            release_callback_gate.store(true, std::memory_order_release);
            seed_subscription.join();
            offline_refresh.join();
            recovery_refresh.join();
            fail(agent, destroy_agent, "Hub did not return the recovery refresh");
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(300));
        const int connected_before_release = connected_callbacks.load();
        release_callback_gate.store(true, std::memory_order_release);
        seed_subscription.join();
        offline_refresh.join();
        recovery_refresh.join();

        if (subscribe_rc != 0 || offline_rc != 0 || recovery_rc != 0 ||
            connected_before_release != 1 || connected_callbacks.load() != 2 ||
            invalid_connected_topics.load() != 0 ||
            disconnected_count(cloud, "studio-serial-1") != 0 ||
            status_count(cloud, "studio-serial-1") < 2 ||
            !contains(status_body(cloud, "studio-serial-1"), R"("mc_percent":73)")) {
            fail(agent, destroy_agent, "callback gate delivered stale or out-of-order printer state");
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }

    if (auth_rejected_mode) {
        const std::string profile = R"({"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"})";
        if (change_user(agent, profile) != 0) {
            fail(agent, destroy_agent, "auth rejection profile setup failed");
        }
        std::atomic<int> ready_callbacks{0};
        std::atomic<int> auth_rejected_callbacks{0};
        std::atomic<int> disconnected_callbacks{0};
        std::atomic<int> local_lost_callbacks{0};
        std::atomic<bool> local_lost_saw_logged_out{false};
        if (set_message(agent, [](std::string, std::string) {}) != 0 ||
            set_server_connected(agent, [&ready_callbacks, &auth_rejected_callbacks, &disconnected_callbacks](int status, int reason) {
                if (status == 0 && reason == 0) ++ready_callbacks;
                else if (status == 5 && reason == 0) ++auth_rejected_callbacks;
                else ++disconnected_callbacks;
            }) != 0 ||
            set_local_connect(agent, [&](int status, std::string, std::string) {
                if (status == 2) {
                    ++local_lost_callbacks;
                    local_lost_saw_logged_out = !is_user_login(agent);
                }
            }) != 0) {
            fail(agent, destroy_agent, "auth rejection callback registration failed");
        }
        std::string printers_body;
        unsigned int printers_code = 0;
        if (get_print_info(agent, &printers_code, &printers_body) != 0 || printers_code != 200 ||
            connect_printer(agent, "studio-serial-1", "127.0.0.1", "user", "pass", false) != 0) {
            fail(agent, destroy_agent, "auth rejection active-printer setup failed");
        }
        if (connect_server(agent) != 0 || !is_server_connected(agent) || ready_callbacks.load() != 1) {
            fail(agent, destroy_agent, "auth rejection readiness setup failed");
        }
        if (set_selected_machine(agent, "studio-serial-1") != 0 ||
            add_subscribe(agent, {"studio-serial-1"}) != 0) {
            fail(agent, destroy_agent, "auth rejection selected-machine setup failed");
        }
        const std::string pushall =
            R"({"pushing":{"command":"pushall","sequence_id":"401","version":1,"push_target":1}})";
        if (send_cloud(agent, "studio-serial-1", pushall, 0, 0) != kBambuConnectFailed ||
            !is_server_connected(agent) || auth_rejected_callbacks.load() != 1 ||
            disconnected_callbacks.load() != 0 || !is_user_login(agent) ||
            get_selected_machine(agent) != "studio-serial-1" || local_lost_callbacks.load() != 0) {
            fail(agent, destroy_agent, "401 did not preserve reachability, login, and report auth code 5");
        }
        if (send_cloud(agent, "studio-serial-1", pushall, 0, 0) != kBambuConnectFailed ||
            !is_server_connected(agent) || auth_rejected_callbacks.load() != 1 ||
            disconnected_callbacks.load() != 0 || !is_user_login(agent)) {
            fail(
                agent,
                destroy_agent,
                "repeated 403 changed reachability, login, or repeated auth callback connected=" +
                    std::to_string(is_server_connected(agent)) + " auth_callbacks=" +
                    std::to_string(auth_rejected_callbacks.load()) + " disconnected_callbacks=" +
                    std::to_string(disconnected_callbacks.load()) + " logged_in=" +
                    std::to_string(is_user_login(agent))
            );
        }
        if (user_logout(agent, false) != 0 || is_user_login(agent) ||
            local_lost_callbacks.load() != 1 || !local_lost_saw_logged_out.load()) {
            fail(agent, destroy_agent, "deferred Studio logout did not clear the active account exactly once");
        }
        if (user_logout(agent, false) != 0 || is_user_login(agent) ||
            !get_selected_machine(agent).empty() || local_lost_callbacks.load() != 1) {
            fail(agent, destroy_agent, "repeated Studio logout was not idempotent");
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }

    if (account_transition_mode) {
        const std::string profile_a = R"({"token":"probe-token","user_id":"account-a","user_name":"Account A","tenant_id":"tenant-a","tenant_name":"Tenant A"})";
        const std::string profile_b = R"({"token":"probe-token","user_id":"account-b","user_name":"Account B","tenant_id":"tenant-b","tenant_name":"Tenant B"})";
        if (change_user(agent, profile_a) != 0) {
            fail(agent, destroy_agent, "account transition A setup failed");
        }
        std::string printers_body;
        unsigned int printers_code = 0;
        if (get_print_info(agent, &printers_code, &printers_body) != 0 || printers_code != 200) {
            fail(agent, destroy_agent, "account transition A printer seed failed");
        }
        TunnelCapture cloud;
        std::atomic<int> printer_callbacks{0};
        std::atomic<int> local_connected_callbacks{0};
        std::atomic<int> local_lost_callbacks{0};
        std::atomic<bool> local_lost_saw_account_b{false};
        std::atomic<int> reentrant_print_rc{999};
        std::atomic<int> reentrant_connect_rc{999};
        if (set_message(agent, [&cloud](std::string dev_id, std::string body) {
                capture_tunnel_message(cloud, dev_id, body);
            }) != 0 ||
            set_printer_connected(agent, [&printer_callbacks](std::string) {
                ++printer_callbacks;
            }) != 0 ||
            set_local_connect(agent, [&](int status, std::string, std::string) {
                if (status == 0) ++local_connected_callbacks;
                if (status == 2) {
                    ++local_lost_callbacks;
                    local_lost_saw_account_b =
                        is_user_login(agent) && get_user_id(agent) == "account-b";
                    add_subscribe(agent, {"studio-serial-1"});
                    unsigned int callback_code = 0;
                    std::string callback_body;
                    reentrant_print_rc = get_print_info(agent, &callback_code, &callback_body);
                    reentrant_connect_rc = connect_printer(
                        agent,
                        "studio-serial-1",
                        "127.0.0.1",
                        "user",
                        "pass",
                        false
                    );
                }
            }) != 0 ||
            connect_printer(agent, "studio-serial-1", "127.0.0.1", "user", "pass", false) != 0) {
            fail(agent, destroy_agent, "account transition active local setup failed");
        }
        if (change_user(agent, profile_b) != 0 || local_lost_callbacks.load() != 1 ||
            !local_lost_saw_account_b.load() ||
            reentrant_print_rc.load() == 0 ||
            reentrant_connect_rc.load() != kBambuConnectFailed ||
            local_connected_callbacks.load() != 1 || printer_callbacks.load() != 0 ||
            status_count(cloud, "studio-serial-1") != 0) {
            fail(agent, destroy_agent, "account A callbacks escaped into same-token account B");
        }
        if (get_print_info(agent, &printers_code, &printers_body) != 0 || printers_code != 200) {
            fail(agent, destroy_agent, "account transition B printer seed failed");
        }
        if (!wait_until([&] {
                return status_count(cloud, "studio-serial-1") >= 1 &&
                    printer_callbacks.load() >= 1;
            })) {
            fail(agent, destroy_agent, "account transition dropped a reentrant subscription");
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }
