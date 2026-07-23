    if (account_exchange_race_mode) {
        const std::string profile_a = R"({"token":"account-a-token","user_id":"account-a","user_name":"Account A","tenant_id":"tenant-a","tenant_name":"Tenant A"})";
        const std::string profile_b = R"({"token":"account-b-token","user_id":"account-b","user_name":"Account B","tenant_id":"tenant-b","tenant_name":"Tenant B"})";
        if (change_user(agent, profile_a) != 0) {
            fail(agent, destroy_agent, "account exchange race initial account setup failed");
        }

        std::string delayed_profile_a;
        unsigned int delayed_profile_code = 0;
        if (get_profile(
                agent,
                "account-a-token",
                &delayed_profile_code,
                &delayed_profile_a
            ) != 0 ||
            delayed_profile_code != 200 ||
            !contains(delayed_profile_a, "account-a") ||
            contains(delayed_profile_a, "\"token\"")) {
            fail(agent, destroy_agent, "account exchange race did not capture a tokenless account A profile");
        }

        std::string printers_body;
        unsigned int printers_code = 0;
        if (get_print_info(agent, &printers_code, &printers_body) != 0 ||
            printers_code != 200) {
            fail(agent, destroy_agent, "account exchange race initial printer seed failed");
        }

        std::atomic<int> valid_login_callbacks{0};
        std::atomic<int> invalid_login_callbacks{0};
        std::atomic<int> http_error_callbacks{0};
        std::atomic<int> local_connected_callbacks{0};
        std::atomic<int> local_lost_callbacks{0};
        std::atomic<bool> callback_account_read_requested{false};
        std::atomic<bool> callback_account_read_finished{false};
        std::atomic<bool> callback_account_read_succeeded{false};
        std::atomic<bool> callback_account_read_completed_inside_callback{false};
        std::atomic<bool> callback_account_reader_started{false};
        std::atomic<bool> callback_account_reader_stop{false};
        std::mutex callback_users_mutex;
        std::vector<std::string> callback_users;
        std::vector<bool> callback_events;
        if (set_user_login(agent, [&](int state, bool login) {
                if ((state == 1 && login) || (state == 0 && !login)) {
                    const int callback_index = valid_login_callbacks.fetch_add(1);
                    if (callback_index == 0) {
                        callback_account_read_requested.store(true, std::memory_order_release);
                        const auto callback_deadline = std::chrono::steady_clock::now() +
                            std::chrono::milliseconds(750);
                        while (!callback_account_read_finished.load(std::memory_order_acquire) &&
                               std::chrono::steady_clock::now() < callback_deadline) {
                            std::this_thread::sleep_for(std::chrono::milliseconds(5));
                        }
                        callback_account_read_completed_inside_callback.store(
                            callback_account_read_finished.load(std::memory_order_acquire),
                            std::memory_order_release
                        );
                    }
                } else {
                    ++invalid_login_callbacks;
                }
                std::lock_guard<std::mutex> lock(callback_users_mutex);
                callback_users.push_back(get_user_id(agent));
                callback_events.push_back(login);
            }) != 0 ||
            set_http_error(agent, [&](unsigned, std::string) {
                ++http_error_callbacks;
            }) != 0 ||
            set_local_connect(agent, [&](int state, std::string, std::string) {
                if (state == 0) ++local_connected_callbacks;
                if (state == 2) ++local_lost_callbacks;
            }) != 0 ||
            connect_printer(
                agent,
                "studio-serial-1",
                "127.0.0.1",
                "user",
                "pass",
                false
            ) != 0 || local_connected_callbacks.load() != 1) {
            fail(agent, destroy_agent, "account exchange race callback setup failed");
        }

        int exchange_rc = -999;
        unsigned int exchange_code = 0;
        std::string exchange_body;
        std::thread stale_exchange([&] {
            exchange_rc = get_token(
                agent,
                "serialized-stale-ticket",
                &exchange_code,
                &exchange_body
            );
        });
        const auto exchange_entered =
            std::filesystem::path(config_dir) / "account-exchange-entered";
        if (!wait_until([&] { return std::filesystem::exists(exchange_entered); })) {
            std::filesystem::create_directory(
                std::filesystem::path(config_dir) / "account-exchange-release"
            );
            stale_exchange.join();
            fail(agent, destroy_agent, "ticket exchange did not enter the blocked Hub response");
        }

        std::atomic<bool> replacement_started{false};
        int replacement_rc = -999;
        std::thread replacement([&] {
            replacement_started.store(true, std::memory_order_release);
            replacement_rc = change_user(agent, profile_b);
        });
        if (!wait_until([&] {
                return replacement_started.load(std::memory_order_acquire);
            })) {
            std::filesystem::create_directory(
                std::filesystem::path(config_dir) / "account-exchange-release"
            );
            stale_exchange.join();
            replacement.join();
            fail(agent, destroy_agent, "later account mutation thread did not start");
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(200));
        const int callbacks_before_release = valid_login_callbacks.load();
        std::thread callback_account_reader([&] {
            callback_account_reader_started.store(true, std::memory_order_release);
            while (!callback_account_read_requested.load(std::memory_order_acquire) &&
                   !callback_account_reader_stop.load(std::memory_order_acquire)) {
                std::this_thread::sleep_for(std::chrono::milliseconds(5));
            }
            if (!callback_account_reader_stop.load(std::memory_order_acquire)) {
                const auto callback_user = get_user_id(agent);
                const auto callback_login_info = build_login_info(agent);
                const bool stale_account_complete =
                    callback_user == "stale-response-user" &&
                    contains(callback_login_info, "stale-response-token") &&
                    !contains(callback_login_info, "account-b-token");
                const bool replacement_account_complete =
                    callback_user == "account-b" &&
                    contains(callback_login_info, "account-b-token") &&
                    !contains(callback_login_info, "stale-response-token");
                callback_account_read_succeeded.store(
                    is_user_login(agent) &&
                        (stale_account_complete || replacement_account_complete),
                    std::memory_order_release
                );
                callback_account_read_finished.store(true, std::memory_order_release);
            }
        });
        const bool callback_reader_ready = wait_until([&] {
            return callback_account_reader_started.load(std::memory_order_acquire);
        });

        std::filesystem::create_directory(
            std::filesystem::path(config_dir) / "account-exchange-release"
        );
        stale_exchange.join();
        replacement.join();
        callback_account_reader_stop.store(true, std::memory_order_release);
        callback_account_reader.join();

        if (callbacks_before_release != 1 || !callback_reader_ready) {
            fail(agent, destroy_agent, "later account mutation did not fence the in-flight ticket exchange");
        }
        std::vector<std::string> observed_users;
        std::vector<bool> observed_events;
        {
            std::lock_guard<std::mutex> lock(callback_users_mutex);
            observed_users = callback_users;
            observed_events = callback_events;
        }
        const auto final_user_id = get_user_id(agent);
        const auto final_login_info = build_login_info(agent);
        const bool final_logged_in = is_user_login(agent);
        if (exchange_rc != kBambuInvalidResult || exchange_code != 409 || replacement_rc != 0 ||
            valid_login_callbacks.load() != 1 || invalid_login_callbacks.load() != 0 ||
            !callback_account_read_completed_inside_callback.load(std::memory_order_acquire) ||
            !callback_account_read_succeeded.load(std::memory_order_acquire) ||
            observed_events != std::vector<bool>{true} ||
            local_lost_callbacks.load() != 1 || !final_logged_in ||
            final_user_id != "account-b" ||
            !contains(final_login_info, "account-b-token") ||
            contains(final_login_info, "stale-response-token")) {
            std::ostringstream diagnostic;
            diagnostic << "serialized account mutations produced stale state or callbacks"
                << " exchange=" << exchange_rc << "/" << exchange_code
                << " replacement=" << replacement_rc
                << " login_callbacks=" << valid_login_callbacks.load()
                << " invalid_callbacks=" << invalid_login_callbacks.load()
                << " account_read_inside="
                << callback_account_read_completed_inside_callback.load()
                << " account_read_ok=" << callback_account_read_succeeded.load()
                << " lost=" << local_lost_callbacks.load()
                << " logged_in=" << final_logged_in
                << " final_user=" << final_user_id
                << " callback_users=";
            for (const auto& user : observed_users) diagnostic << user << ",";
            fail(agent, destroy_agent, diagnostic.str());
        }

        const auto persisted_login_path =
            std::filesystem::path(config_dir) / "pandar-plugin-login.json";
        const auto read_persisted_login = [&]() {
            std::ifstream stream(persisted_login_path);
            std::stringstream contents;
            contents << stream.rdbuf();
            return contents.str();
        };
        const auto runtime_before_delayed_profile = build_login_info(agent);
        const auto persisted_before_delayed_profile = read_persisted_login();
        const int login_callbacks_before_delayed_profile = valid_login_callbacks.load();
        const int http_callbacks_before_delayed_profile = http_error_callbacks.load();
        const int lost_callbacks_before_delayed_profile = local_lost_callbacks.load();
        const int delayed_profile_rc = change_user(agent, delayed_profile_a);
        if (delayed_profile_rc != kBambuInvalidResult ||
            valid_login_callbacks.load() != login_callbacks_before_delayed_profile ||
            http_error_callbacks.load() != http_callbacks_before_delayed_profile ||
            local_lost_callbacks.load() != lost_callbacks_before_delayed_profile ||
            get_user_id(agent) != "account-b" ||
            build_login_info(agent) != runtime_before_delayed_profile ||
            read_persisted_login() != persisted_before_delayed_profile) {
            fail(agent, destroy_agent, "late tokenless account A profile changed account B state");
        }

        std::string profile_body;
        unsigned int profile_code = 0;
        if (get_profile(agent, "account-b-token", &profile_code, &profile_body) != 0 ||
            profile_code != 200 || !contains(profile_body, "account-b") ||
            contains(profile_body, "stale-response-user") ||
            get_print_info(agent, &printers_code, &printers_body) != 0 ||
            printers_code != 200 ||
            connect_printer(
                agent,
                "studio-serial-1",
                "127.0.0.1",
                "user",
                "pass",
                false
            ) != 0 || local_connected_callbacks.load() != 2 ||
            local_lost_callbacks.load() != 1) {
            fail(agent, destroy_agent, "later account state did not remain operational");
        }

        std::string stale_profile_body;
        unsigned int stale_profile_code = 0;
        const auto profile_state_before = build_login_info(agent);
        if (get_profile(agent, "account-a-token", &stale_profile_code, &stale_profile_body) !=
                kBambuInvalidResult ||
            stale_profile_code != 409 ||
            !contains(stale_profile_body, "stale_account_response") ||
            get_user_id(agent) != "account-b" ||
            build_login_info(agent) != profile_state_before) {
            fail(agent, destroy_agent, "stale profile token replaced the current account");
        }

        void* restored_agent = create_agent("account-race-restore");
        const bool replacement_persisted =
            restored_agent && set_config_dir(restored_agent, config_dir) == 0 &&
            is_user_login(restored_agent) && get_user_id(restored_agent) == "account-b" &&
            contains(build_login_info(restored_agent), "account-b-token") &&
            !contains(build_login_info(restored_agent), "stale-response-token");
        if (restored_agent) destroy_agent(restored_agent);
        if (!replacement_persisted) {
            fail(agent, destroy_agent, "old ticket response overwrote persisted account state");
        }

        std::atomic<int> fifo_callbacks{0};
        std::atomic<int> malformed_fifo_callbacks{0};
        std::mutex fifo_events_mutex;
        std::vector<bool> fifo_events;
        if (set_user_login(agent, [&](int state, bool login) {
                if ((state == 1 && login) || (state == 0 && !login)) {
                    ++fifo_callbacks;
                } else {
                    ++malformed_fifo_callbacks;
                }
                std::lock_guard<std::mutex> lock(fifo_events_mutex);
                fifo_events.push_back(login);
            }) != 0) {
            fail(agent, destroy_agent, "FIFO account callback registration failed");
        }

        int fifo_login_rc = -999;
        unsigned int fifo_login_code = 0;
        std::string fifo_login_body;
        std::thread fifo_login([&] {
            fifo_login_rc = get_token(
                agent,
                "fifo-login-ticket",
                &fifo_login_code,
                &fifo_login_body
            );
        });
        const auto fifo_exchange_entered =
            std::filesystem::path(config_dir) / "account-fifo-exchange-entered";
        if (!wait_until([&] { return std::filesystem::exists(fifo_exchange_entered); })) {
            std::filesystem::create_directory(
                std::filesystem::path(config_dir) / "account-fifo-exchange-release"
            );
            fifo_login.join();
            fail(agent, destroy_agent, "FIFO login exchange did not enter the blocked response");
        }

        std::atomic<bool> fifo_logout_started{false};
        std::atomic<bool> fifo_logout_finished{false};
        int fifo_logout_rc = -999;
        std::thread fifo_logout([&] {
            fifo_logout_started.store(true, std::memory_order_release);
            fifo_logout_rc = user_logout(agent, false);
            fifo_logout_finished.store(true, std::memory_order_release);
        });
        const bool fifo_logout_thread_started = wait_until([&] {
            return fifo_logout_started.load(std::memory_order_acquire);
        });
        std::this_thread::sleep_for(std::chrono::milliseconds(200));
        const bool fifo_logout_completed_while_login_blocked =
            fifo_logout_finished.load(std::memory_order_acquire);
        std::filesystem::create_directory(
            std::filesystem::path(config_dir) / "account-fifo-exchange-release"
        );
        fifo_login.join();
        fifo_logout.join();

        const auto pending_revoke_file = std::filesystem::path(config_dir) /
            "pandar-plugin-pending-revocations.json";
        std::ifstream pending_revoke_stream(pending_revoke_file);
        std::stringstream pending_revoke_contents;
        pending_revoke_contents << pending_revoke_stream.rdbuf();
        const auto pending_revoke_body = pending_revoke_contents.str();
        const bool failed_candidate_revoke_remained_durable =
            pending_revoke_stream.is_open() &&
            contains(pending_revoke_body, "fifo-login-token") &&
            !contains(pending_revoke_body, "stale-response-token");

        std::vector<bool> observed_fifo_events;
        {
            std::lock_guard<std::mutex> lock(fifo_events_mutex);
            observed_fifo_events = fifo_events;
        }
        if (!fifo_logout_thread_started || !fifo_logout_completed_while_login_blocked ||
            fifo_login_rc != kBambuInvalidResult || fifo_login_code != 409 || fifo_logout_rc != 0 ||
            fifo_callbacks.load() != 1 || malformed_fifo_callbacks.load() != 0 ||
            observed_fifo_events != std::vector<bool>{false} ||
            !failed_candidate_revoke_remained_durable ||
            is_user_login(agent) || !get_user_id(agent).empty() ||
            contains(build_login_info(agent), "fifo-login-token")) {
            fail(agent, destroy_agent, "account callback queue did not preserve login-before-logout FIFO");
        }

        if (destroy_agent(agent) != 0) {
            fail(nullptr, destroy_agent, "account exchange race destroy failed");
        }
        agent = create_agent("account-race-logout-restore");
        if (!agent || set_config_dir(agent, config_dir) != 0 || is_user_login(agent)) {
            fail(agent, destroy_agent, "FIFO logout did not clear persisted account state");
        }
        std::atomic<int> logged_out_http_callbacks{0};
        if (set_http_error(agent, [&](unsigned, std::string) {
                ++logged_out_http_callbacks;
            }) != 0) {
            fail(agent, destroy_agent, "logged-out profile callback setup failed");
        }
        std::string logged_out_profile_body;
        unsigned int logged_out_profile_code = 0;
        if (get_profile(
                agent,
                "account-a-token",
                &logged_out_profile_code,
                &logged_out_profile_body
            ) != kBambuInvalidResult ||
            logged_out_profile_code != 409 ||
            !contains(logged_out_profile_body, "stale_account_response") ||
            logged_out_http_callbacks.load() != 0) {
            fail(agent, destroy_agent, "logged-out stale profile token reported availability instead of staleness");
        }
        if (get_profile(
                agent,
                "",
                &logged_out_profile_code,
                &logged_out_profile_body
            ) != kBambuInvalidResult ||
            logged_out_profile_code != 401 ||
            !contains(logged_out_profile_body, "profile_unavailable") ||
            logged_out_http_callbacks.load() != 1) {
            fail(agent, destroy_agent, "logged-out empty profile token did not report availability once");
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }
