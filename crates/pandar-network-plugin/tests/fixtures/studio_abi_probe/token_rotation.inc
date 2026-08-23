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
        const std::string rotated_profile =
            R"({"token":"rotated-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"})";
        if (change_user(agent, rotated_profile) != 0 ||
            !wait_until([&] {
                return std::filesystem::exists(
                    std::filesystem::path(config_dir) / "rotation-rotated-served"
                );
            })) {
            fail(agent, destroy_agent, "rotated snapshot was not served");
        }
        // change_user cleared cloud targeting; restore it so the armed
        // removal surfaces the cloud offline delivery exactly once.
        if (add_subscribe(agent, {"studio-serial-1"}) != 0) {
            fail(agent, destroy_agent, "post-rotation cloud subscribe failed");
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(200));
        const int cloud_status_before = status_count(cloud, "studio-serial-1");
        const int local_status_before = status_count(local, "studio-serial-1");
        const int local_offline_before = disconnected_count(local, "studio-serial-1");
        const int local_lost_before = local_lost_callbacks.load();
        std::filesystem::create_directory(
            std::filesystem::path(config_dir) / "rotation-invalid-arm"
        );
        const bool removal_delivered = wait_until([&] {
            return disconnected_count(cloud, "studio-serial-1") == 1 &&
                local_callback_clear_rc.load() == 0;
        });
        const std::string pushall =
            R"({"pushing":{"command":"pushall","sequence_id":"rotation-offline","version":1,"push_target":1}})";
        if (!removal_delivered ||
            send_cloud(agent, "studio-serial-1", pushall, 0, 0) != kBambuConnectFailed ||
            !contains(build_login_info(agent), "rotated-token") ||
            auth_rejected_callbacks.load() != 0 || local_connected_callbacks.load() != 1 ||
            local_lost_callbacks.load() != local_lost_before ||
            disconnected_count(local, "studio-serial-1") != local_offline_before ||
            status_count(cloud, "studio-serial-1") != cloud_status_before ||
            status_count(local, "studio-serial-1") != local_status_before ||
            send_printer(agent, "studio-serial-1", "M999", 0, 0) != kBambuConnectFailed) {
            fail(agent, destroy_agent, "offline token retry leaked stale presence or local send");
        }
        if (send_cloud(agent, "studio-serial-1", pushall, 0, 0) != kBambuConnectFailed ||
            disconnected_count(cloud, "studio-serial-1") != 1 ||
            disconnected_count(local, "studio-serial-1") != local_offline_before ||
            local_lost_callbacks.load() != local_lost_before) {
            fail(agent, destroy_agent, "offline token retry repeated disconnect delivery");
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }

    if (token_rotation_mode) {
        if (set_selected_machine(agent, "") != 0) {
            fail(agent, destroy_agent, "token rotation selected-target reset failed");
        }
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
        if (!wait_until([&] { return printer_callbacks.load() == 1; })) {
            fail(agent, destroy_agent, "reentrant unsubscribe callback was missing");
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(200));
        const int status_before_subscribe = status_count(cloud, "studio-serial-1");
        const int printers_before_subscribe = printer_callbacks.load();
        if (add_subscribe(agent, {"studio-serial-1"}) != 0) {
            fail(agent, destroy_agent, "selected target subscription restore failed");
        }
        if (!wait_until([&] {
                return status_count(cloud, "studio-serial-1") == status_before_subscribe + 1 &&
                    printer_callbacks.load() == printers_before_subscribe + 1;
            })) {
            fail(agent, destroy_agent, "restored subscription did not emit fresh status");
        }
        const int status_before_selected_push = status_count(cloud, "studio-serial-1");
        const std::string selected_pushall =
            R"({"pushing":{"command":"pushall","sequence_id":"selected-after-unsubscribe","version":1,"push_target":1}})";
        if (send_cloud(agent, "studio-serial-1", selected_pushall, 0, 0) != 0 ||
            status_count(cloud, "studio-serial-1") != status_before_selected_push + 1 ||
            connect_printer(
                agent,
                "studio-serial-1",
                "127.0.0.1",
                "user",
                "pass",
                false
            ) != 0) {
            fail(
                agent,
                destroy_agent,
                "reentrant unsubscribe did not fence and restore status printers=" +
                    std::to_string(printer_callbacks.load()) + " status=" +
                    std::to_string(status_count(cloud, "studio-serial-1"))
            );
        }
        const std::string rotated_profile =
            R"({"token":"rotated-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"})";
        if (change_user(agent, rotated_profile) != 0 ||
            !wait_until([&] {
                return std::filesystem::exists(
                    std::filesystem::path(config_dir) / "rotation-rotated-served"
                );
            })) {
            fail(agent, destroy_agent, "rotated snapshot was not served");
        }
        // Stream era: the account swap fences active local state (exactly one
        // local lost) and clears cloud targeting; re-establish both and the
        // rotated snapshot must then serve fresh status again.
        if (add_subscribe(agent, {"studio-serial-1"}) != 0 ||
            set_selected_machine(agent, "studio-serial-1") != 0 ||
            connect_printer(agent, "studio-serial-1", "127.0.0.1", "user", "pass", false) != 0) {
            fail(agent, destroy_agent, "post-rotation active state restore failed");
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(200));
        const int status_before_rotation = status_count(cloud, "studio-serial-1");
        const std::string pushall =
            R"({"pushing":{"command":"pushall","sequence_id":"rotation","version":1,"push_target":1}})";
        if (send_cloud(agent, "studio-serial-1", pushall, 0, 0) != 0 ||
            !wait_until([&] {
                return status_count(cloud, "studio-serial-1") == status_before_rotation + 1;
            }) ||
            !contains(build_login_info(agent), "rotated-token") ||
            get_selected_machine(agent) != "studio-serial-1" ||
            auth_rejected_callbacks.load() != 0 || local_lost_callbacks.load() != 1 ||
            local_connected_callbacks.load() != 2) {
            fail(
                agent,
                destroy_agent,
                "successful token rotation changed active printer state or emitted code 5 status=" +
                    std::to_string(status_count(cloud, "studio-serial-1")) + "/" +
                    std::to_string(status_before_rotation) + " auth=" +
                    std::to_string(auth_rejected_callbacks.load()) + " lost=" +
                    std::to_string(local_lost_callbacks.load()) + " local=" +
                    std::to_string(local_connected_callbacks.load()) + " selected=" +
                    get_selected_machine(agent) + " login=" + build_login_info(agent)
            );
        }
        const std::string get_version =
            R"({"info":{"command":"get_version","sequence_id":"rotation-version"}})";
        if (send_printer(agent, "studio-serial-1", get_version, 0, 0) != 0 ||
            !wait_until([&] { return version_count(local, "studio-serial-1") == 1; })) {
            fail(agent, destroy_agent, "successful token rotation lost the active local generation");
        }
        std::filesystem::create_directory(
            std::filesystem::path(config_dir) / "rotation-invalid-arm"
        );
        const int status_before_invalid = status_count(cloud, "studio-serial-1");
        const int cloud_offline_before = disconnected_count(cloud, "studio-serial-1");
        const int local_offline_before = disconnected_count(local, "studio-serial-1");
        const int local_lost_before = local_lost_callbacks.load();
        const bool removal_delivered = wait_until([&] {
            return disconnected_count(cloud, "studio-serial-1") == cloud_offline_before + 1 &&
                disconnected_count(local, "studio-serial-1") == local_offline_before + 1 &&
                local_lost_callbacks.load() == local_lost_before + 1;
        });
        const int invalid_push_rc = send_cloud(agent, "studio-serial-1", pushall, 0, 0);
        if (!removal_delivered || invalid_push_rc != kBambuConnectFailed ||
            status_count(cloud, "studio-serial-1") != status_before_invalid ||
            auth_rejected_callbacks.load() != 0 || !is_server_connected(agent)) {
            fail(
                agent,
                destroy_agent,
                "invalid rotated status replayed stale presence or changed Hub reachability push_rc=" +
                    std::to_string(invalid_push_rc) +
                    " cloud_offline=" +
                    std::to_string(disconnected_count(cloud, "studio-serial-1")) +
                    " local_offline=" +
                    std::to_string(disconnected_count(local, "studio-serial-1")) +
                    " lost=" + std::to_string(local_lost_callbacks.load()) +
                    " status=" + std::to_string(status_count(cloud, "studio-serial-1")) +
                    "/" + std::to_string(status_before_invalid) + " auth=" +
                    std::to_string(auth_rejected_callbacks.load()) + " connected=" +
                    std::to_string(is_server_connected(agent))
            );
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }
