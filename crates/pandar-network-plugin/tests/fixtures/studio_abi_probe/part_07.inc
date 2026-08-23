    if (native_print_error_mode) {
        if (connect_server(agent) != 0 || !is_server_connected(agent)) {
            fail(agent, destroy_agent, "native probe could not establish healthy server state");
        }
        TunnelCapture cloud;
        TunnelCapture local;
        std::atomic<int> printer_connected_callbacks{0};
        std::atomic<int> local_connect_callbacks{0};
        std::atomic<int> server_connect_callbacks{0};
        auto cloud_callback = [&cloud](std::string dev_id, std::string body) {
            capture_tunnel_message(cloud, dev_id, body);
        };
        auto local_callback = [&local](std::string dev_id, std::string body) {
            capture_tunnel_message(local, dev_id, body);
        };
        if (set_message(agent, cloud_callback) != 0 ||
            set_local_message(agent, local_callback) != 0 ||
            set_printer_connected(agent, [&printer_connected_callbacks](std::string) {
                ++printer_connected_callbacks;
            }) != 0 ||
            set_local_connect(agent, [&local_connect_callbacks](int status, std::string, std::string) {
                if (status == 0) ++local_connect_callbacks;
            }) != 0 ||
            set_server_connected(agent, [&server_connect_callbacks](int, int) {
                ++server_connect_callbacks;
            }) != 0) {
            fail(agent, destroy_agent, "native callback registration failed");
        }
        del_subscribe(agent, {"studio-serial-2", "studio-serial-3"});
        add_subscribe(agent, {"studio-serial-1"});

        const int cloud_version_before = cloud.version_messages.load();
        const int cloud_status_before = cloud.status_messages.load();
        const int local_version_before = local.version_messages.load();
        const int local_status_before = local.status_messages.load();
        const int cloud_version_rc = send_cloud(
            agent,
            "studio-serial-1",
            R"({"info":{"command":"get_version","sequence_id":"30001"}})",
            0,
            0
        );
        const bool cloud_version_exact =
            cloud_version_rc == 0 &&
            cloud.version_messages.load() == cloud_version_before + 1 &&
            cloud.status_messages.load() == cloud_status_before &&
            local.version_messages.load() == local_version_before &&
            local.status_messages.load() == local_status_before;

        const int cloud_status_request_before = cloud.status_messages.load();
        const int cloud_version_after_get = cloud.version_messages.load();
        const int cloud_push_rc = send_cloud(
            agent,
            "studio-serial-1",
            R"({"pushing":{"command":"pushall","sequence_id":"30002","version":1,"push_target":1}})",
            0,
            0
        );
        const bool cloud_push_exact =
            cloud_push_rc == 0 &&
            cloud.status_messages.load() == cloud_status_request_before + 1 &&
            cloud.version_messages.load() == cloud_version_after_get &&
            local.version_messages.load() == local_version_before &&
            local.status_messages.load() == local_status_before;
        const int cloud_replay_before = status_count(cloud, "studio-serial-1");
        std::this_thread::sleep_for(std::chrono::milliseconds(2'300));
        const bool cloud_no_periodic_replay =
            status_count(cloud, "studio-serial-1") == cloud_replay_before;
        out.cloud_requests_exact =
            cloud_version_exact && cloud_push_exact && cloud_no_periodic_replay;

        del_subscribe(agent, {"studio-serial-2"});
        send_cloud(
            agent,
            "studio-serial-1",
            R"({"pushing":{"command":"pushall","sequence_id":"30003","version":1,"push_target":1}})",
            0,
            0
        );
        const int local_cloud_before = status_count(cloud, "studio-serial-2");
        const std::string selected_before_local = get_selected_machine(agent);
        const int printer_connected_before = printer_connected_callbacks.load();
        const int local_connect_before = local_connect_callbacks.load();
        const int cloud_messages_before_connect =
            cloud.version_messages.load() + cloud.status_messages.load();
        const int local_messages_before_connect =
            local.version_messages.load() + local.status_messages.load();
        const int local_connect_rc = connect_printer(
            agent,
            "studio-serial-2",
            "127.0.0.1",
            "user",
            "pass",
            false
        );
        out.lan_connect_exact =
            local_connect_rc == 0 &&
            local_connect_callbacks.load() == local_connect_before + 1 &&
            printer_connected_callbacks.load() == printer_connected_before &&
            cloud.version_messages.load() + cloud.status_messages.load() == cloud_messages_before_connect &&
            local.version_messages.load() + local.status_messages.load() == local_messages_before_connect &&
            get_selected_machine(agent) == selected_before_local;
        set_selected_machine(agent, selected_before_local);

        const int operation_posts_before = hub_operation_count();
        const int local_version_request_before = version_count(local, "studio-serial-2");
        const int local_status_request_before = status_count(local, "studio-serial-2");
        const int cloud_messages_before_local_requests =
            cloud.version_messages.load() + cloud.status_messages.load();
        const int local_version_rc = send_printer(
            agent,
            "studio-serial-2",
            R"({"info":{"command":"get_version","sequence_id":"30004"}})",
            0,
            0
        );
        const bool local_version_exact =
            local_version_rc == 0 &&
            version_count(local, "studio-serial-2") == local_version_request_before + 1 &&
            status_count(local, "studio-serial-2") == local_status_request_before &&
            cloud.version_messages.load() + cloud.status_messages.load() == cloud_messages_before_local_requests;
        const int local_version_after_get = version_count(local, "studio-serial-2");
        const int local_push_rc = send_printer(
            agent,
            "studio-serial-2",
            R"({"pushing":{"command":"pushall","sequence_id":"30005","version":1,"push_target":1}})",
            0,
            0
        );
        const bool local_push_exact =
            local_push_rc == 0 &&
            status_count(local, "studio-serial-2") == local_status_request_before + 1 &&
            version_count(local, "studio-serial-2") == local_version_after_get &&
            cloud.version_messages.load() + cloud.status_messages.load() == cloud_messages_before_local_requests;
        const int local_replay_before = status_count(local, "studio-serial-2");
        std::this_thread::sleep_for(std::chrono::milliseconds(2'300));
        const bool local_no_periodic_replay =
            status_count(local, "studio-serial-2") == local_replay_before;
        const bool local_stayed_out_of_cloud =
            status_count(cloud, "studio-serial-2") == local_cloud_before;
        const int operation_posts_after = hub_operation_count();
        out.local_requests_exact =
            local_version_exact &&
            local_push_exact &&
            local_no_periodic_replay &&
            operation_posts_before == operation_posts_after;
        out.local_only_refresh_exact =
            local_no_periodic_replay && local_stayed_out_of_cloud;
        out.local_status_body = status_body(local, "studio-serial-2");

        out.status_requests_zero_posts = hub_operation_count() == 0;
        const std::vector<std::string> cloud_native_actions = {
            R"({"print":{"command":"resume","err":"83918929","job_id":"cloud-resume-get_version-pushall","param":"reserve","sequence_id":"20042"}})",
            R"({"print":{"command":"ignore","err":"83918929","job_id":"cloud-ignore-get_version-pushall","param":"reserve","sequence_id":"20043"}})",
            R"({"print":{"command":"stop","err":"83918929","job_id":"cloud-stop-get_version-pushall","param":"reserve","sequence_id":"20044"}})",
        };
        const std::vector<std::string> local_native_actions = {
            R"({"print":{"command":"resume","err":"83918929","job_id":"local-resume-get_version-pushall","param":"reserve","sequence_id":"20045"}})",
            R"({"print":{"command":"ignore","err":"83918929","job_id":"local-ignore-get_version-pushall","param":"reserve","sequence_id":"20046"}})",
            R"({"print":{"command":"stop","err":"83918929","job_id":"local-stop-get_version-pushall","param":"reserve","sequence_id":"20047"}})",
        };
        auto valid_actions_exact = [&](auto send, const std::string& dev_id, const std::vector<std::string>& messages) {
            bool exact = true;
            for (const auto& native_message : messages) {
                const int before = hub_operation_count();
                const bool connected_before = is_server_connected(agent);
                const int rc = send(agent, dev_id, native_message, 1, 0);
                const int after = hub_operation_count();
                exact = exact && before >= 0 && connected_before && rc == 0 &&
                    after == before + 1 && is_server_connected(agent);
            }
            return exact;
        };
        out.native_actions_exact =
            valid_actions_exact(send_cloud, "studio-serial-1", cloud_native_actions) &&
            valid_actions_exact(send_printer, "studio-serial-2", local_native_actions);

        auto callback_count = [&] {
            return cloud.version_messages.load() + cloud.status_messages.load() +
                local.version_messages.load() + local.status_messages.load() +
                printer_connected_callbacks.load() + local_connect_callbacks.load() +
                server_connect_callbacks.load();
        };
        const int empty_target_refresh_before = hub_printer_refresh_count();
        const int empty_target_posts_before = hub_operation_count();
        const int empty_target_callbacks_before = callback_count();
        set_selected_machine(agent, "studio-serial-1");
        const int empty_target_rc = send_cloud(
            agent,
            "",
            R"({"print":{"command":"pause","sequence_id":"blank-target"}})",
            1,
            0
        );
        const int empty_target_callbacks_after = callback_count();
        out.empty_cloud_target_rejected_without_refresh_post_or_callbacks =
            empty_target_refresh_before >= 0 && empty_target_posts_before >= 0 &&
            get_selected_machine(agent) == "studio-serial-1" &&
            empty_target_rc == kBambuConnectFailed &&
            hub_printer_refresh_count() == empty_target_refresh_before &&
            hub_operation_count() == empty_target_posts_before &&
            empty_target_callbacks_after == empty_target_callbacks_before &&
            is_server_connected(agent);
        if (!out.empty_cloud_target_rejected_without_refresh_post_or_callbacks) {
            out.command_disposition_failure = "empty_cloud_dev_id";
        }
