    if (axis_features_mode) {
        if (connect_server(agent) != 0 || !is_server_connected(agent)) {
            fail(agent, destroy_agent, "axis feature probe could not establish healthy server state");
        }
        TunnelCapture cloud;
        TunnelCapture local;
        std::atomic<int> local_connect_callbacks{0};
        if (set_message(agent, [&cloud](std::string dev_id, std::string body) {
                capture_tunnel_message(cloud, dev_id, body);
            }) != 0 ||
            set_local_message(agent, [&local](std::string dev_id, std::string body) {
                capture_tunnel_message(local, dev_id, body);
            }) != 0 ||
            set_local_connect(agent, [&local_connect_callbacks](int status, std::string, std::string) {
                if (status == 0) ++local_connect_callbacks;
            }) != 0 ||
            add_subscribe(agent, {"studio-serial-1"}) != 0) {
            fail(agent, destroy_agent, "axis feature callback registration failed");
        }
        if (send_cloud(
                agent,
                "studio-serial-1",
                R"({"pushing":{"command":"pushall","sequence_id":"31000","version":1,"push_target":1}})",
                0,
                0
            ) != 0 ||
            !wait_until([&] { return status_count(cloud, "studio-serial-1") > 0; })) {
            fail(agent, destroy_agent, "axis feature push_status request failed");
        }
        out.cloud_status_body = status_body(cloud, "studio-serial-1");
        if (!contains(out.cloud_status_body, R"("fun":"8000004100000020")")) {
            fail(agent, destroy_agent, "Studio push_status did not preserve the full u64 fun bitmap");
        }
        if (connect_printer(agent, "studio-serial-2", "127.0.0.1", "user", "pass", false) != 0 ||
            local_connect_callbacks.load() != 1) {
            fail(agent, destroy_agent, "axis feature local printer connect failed");
        }

        const std::vector<std::string> cloud_operations = {
            R"({"print":{"command":"back_to_center","sequence_id":"31001"}})",
            R"({"print":{"command":"xyz_ctrl","axis":"X","dir":1,"mode":0,"sequence_id":"31002"}})",
            R"({"print":{"command":"gcode_line","param":"G28 X\n","sequence_id":"31003"}})",
            R"({"print":{"command":"gcode_line","param":"M211 S\nM211 X1 Y1 Z1\nM1002 push_ref_mode\nG91\nG1 X10.0 F3000\nM1002 pop_ref_mode\nM211 R\n","sequence_id":"31004"}})",
            R"({"print":{"command":"gcode_line","param":"M106 P1 S127 \n","sequence_id":"31009"}})",
        };
        const std::vector<std::string> local_operations = {
            R"({"print":{"command":"back_to_center","sequence_id":"31005"}})",
            R"({"print":{"command":"xyz_ctrl","axis":"Z","dir":-1,"mode":1,"sequence_id":"31006"}})",
            R"({"print":{"command":"gcode_line","param":"G28 X\n","sequence_id":"31007"}})",
            R"({"print":{"command":"gcode_line","param":"M211 S\nM211 X1 Y1 Z1\nM1002 push_ref_mode\nG91\nG1 Z-1.0 F600\nM1002 pop_ref_mode\nM211 R\n","sequence_id":"31008"}})",
            R"({"print":{"command":"gcode_line","param":"M620 C1 \r\n; keep trailing  \n\n","sequence_id":"31010"}})",
        };
        auto submit_exact = [&](auto send, const std::string& dev_id, const std::vector<std::string>& messages) {
            for (const auto& message : messages) {
                const int before = hub_operation_count();
                const int rc = send(agent, dev_id, message, 1, 0);
                const int after = hub_operation_count();
                if (before < 0 || rc != 0 || after != before + 1) return false;
            }
            return true;
        };
        out.axis_features_exact =
            submit_exact(send_cloud, "studio-serial-1", cloud_operations) &&
            submit_exact(send_printer, "studio-serial-2", local_operations);
        out.operation_posts = hub_operation_count();
        if (!out.axis_features_exact || out.operation_posts != 10) {
            fail(agent, destroy_agent, "axis feature commands did not submit exact semantic operations");
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }
