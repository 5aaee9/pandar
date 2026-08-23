        struct UnsupportedCommandCase {
            const char* name;
            const char* message;
            bool status_lookalike;
            bool mixed_status;
        };
        const std::vector<UnsupportedCommandCase> unsupported_commands = {
            {"unknown_envelope", R"({"future":{"command":"future_control","sequence_id":"123"}})", false, false},
            {"skip_objects", R"({"print":{"command":"skip_objects","obj_list":[1,2],"sequence_id":"123"}})", false, false},
            {"set_fan", R"({"print":{"command":"set_fan","sequence_id":"123","fan_index":1,"speed":100}})", false, false},
            {"set_airduct", R"({"print":{"sequence_id":"123","command":"set_airduct","modeId":1,"submode":0}})", false, false},
            {"camera", R"({"camera":{"command":"ipcam_record_set","sequence_id":"123","control":"enable"}})", false, false},
            {"xcam", R"({"xcam":{"command":"xcam_control_set","sequence_id":"123","module_name":"fod_check","control":true,"enable":true,"print_halt":true}})", false, false},
            {"buzzer", R"({"print":{"command":"buzzer_ctrl","mode":0,"sequence_id":"123"}})", false, false},
            {"calibration", R"({"print":{"command":"calibration","sequence_id":"123","option":127}})", false, false},
            {"advanced_ams", R"({"print":{"command":"ams_filament_drying","sequence_id":"123","ams_id":0,"mode":1,"filament":"PLA","temp":55,"duration":8,"humidity":0,"rotate_tray":true,"cooling_temp":35,"close_power_conflict":false}})", false, false},
            {"malformed_ordinary", R"({"print":{"command":"set_nozzle_temp","extruder_index":"left","target_temp":245,"sequence_id":"123"}})", false, false},
            {"unknown_pushall", R"({"pushing":{"command":"not_pushall","sequence_id":"30008"}})", true, false},
            {"unknown_get_version", R"({"info":{"command":"not_get_version","sequence_id":"30009"}})", true, false},
            {"mixed_info_pushing", R"({"info":{"command":"get_version","sequence_id":"31001"},"pushing":{"command":"pushall","sequence_id":"31002","version":1,"push_target":1}})", true, true},
            {"mixed_info_print", R"({"info":{"command":"get_version","sequence_id":"31003"},"print":{"command":"pause","sequence_id":"31004"}})", true, true},
            {"mixed_pushing_print", R"({"pushing":{"command":"pushall","sequence_id":"31005","version":1,"push_target":1},"print":{"command":"pause","sequence_id":"31006"}})", true, true},
            {"extra_info_future", R"({"info":{"command":"get_version","sequence_id":"31007"},"future":{"command":"future_control"}})", true, true},
            {"mixed_system_print", R"({"system":{"command":"ledctrl","led_node":"chamber_light","led_mode":"on","sequence_id":"123"},"print":{"command":"pause","sequence_id":"124"}})", false, false},
        };
        const int unsupported_posts_before = hub_operation_count();
        const int unsupported_callbacks_before = callback_count();
        bool cloud_invalid = true;
        bool local_invalid = true;
        bool parser_error_stable = true;
        bool status_invalid_without_callbacks = true;
        bool mixed_status_invalid_without_callbacks = true;
        for (const auto& entry : unsupported_commands) {
            const int callbacks_before = callback_count();
            const int cloud_rc = send_cloud(agent, "studio-serial-1", entry.message, 1, 0);
            const int local_rc = send_printer(agent, "studio-serial-2", entry.message, 1, 0);
            const bool callbacks_unchanged = callback_count() == callbacks_before;
            cloud_invalid = cloud_invalid && cloud_rc == kBambuInvalidResult;
            local_invalid = local_invalid && local_rc == kBambuInvalidResult;
            if (entry.status_lookalike) {
                status_invalid_without_callbacks =
                    status_invalid_without_callbacks &&
                    cloud_rc == kBambuInvalidResult &&
                    local_rc == kBambuInvalidResult &&
                    callbacks_unchanged;
            }
            if (entry.mixed_status) {
                mixed_status_invalid_without_callbacks =
                    mixed_status_invalid_without_callbacks &&
                    cloud_rc == kBambuInvalidResult &&
                    local_rc == kBambuInvalidResult &&
                    callbacks_unchanged;
            }

            const std::string message(entry.message);
            auto parsed = parse_operation(
                reinterpret_cast<const uint8_t*>(message.data()),
                message.size()
            );
            std::string parser_body;
            if (parsed.body_ptr && parsed.body_len > 0) {
                parser_body.assign(
                    reinterpret_cast<char*>(parsed.body_ptr),
                    parsed.body_len
                );
                free_with_capacity(parsed.body_ptr, parsed.body_len, parsed.body_cap);
            }
            parser_error_stable =
                parser_error_stable &&
                parsed.status != 0 &&
                parsed.http_code == 400 &&
                parser_body == R"({"error":"unsupported_printer_operation"})";
            if ((cloud_rc != kBambuInvalidResult ||
                 local_rc != kBambuInvalidResult ||
                 !callbacks_unchanged) &&
                out.command_disposition_failure.empty()) {
                out.command_disposition_failure = entry.name;
            }
        }
        const bool unsupported_no_side_effects =
            unsupported_posts_before >= 0 &&
            hub_operation_count() == unsupported_posts_before &&
            callback_count() == unsupported_callbacks_before &&
            is_server_connected(agent);
        out.cloud_unsupported_exact = cloud_invalid;
        out.local_unsupported_exact = local_invalid;
        out.status_request_lookalikes_exact =
            status_invalid_without_callbacks && unsupported_no_side_effects;
        out.unsupported_commands_rejected_without_post_or_callbacks =
            cloud_invalid && local_invalid && unsupported_no_side_effects;
        out.unsupported_parser_error_stable = parser_error_stable;
        out.mixed_status_envelopes_rejected_without_callbacks =
            mixed_status_invalid_without_callbacks && unsupported_no_side_effects;

        const std::string invalid_native =
            R"({"print":{"command":"resume","err":"0","job_id":"","param":"reserve","sequence_id":"20048"}})";
        int before_outcome = hub_operation_count();
        const bool cloud_invalid_connected_before = is_server_connected(agent);
        const int cloud_invalid_rc = send_cloud(
            agent,
            "studio-serial-1",
            invalid_native,
            1,
            0
        );
        out.cloud_invalid_native_exact =
            before_outcome >= 0 && cloud_invalid_connected_before &&
            cloud_invalid_rc == kBambuInvalidResult &&
            hub_operation_count() == before_outcome && is_server_connected(agent);

        before_outcome = hub_operation_count();
        const bool local_invalid_connected_before = is_server_connected(agent);
        const int local_invalid_rc = send_printer(
            agent,
            "studio-serial-2",
            invalid_native,
            1,
            0
        );
        out.local_invalid_native_exact =
            before_outcome >= 0 && local_invalid_connected_before &&
            local_invalid_rc == kBambuInvalidResult &&
            hub_operation_count() == before_outcome && is_server_connected(agent);

        const int local_messages_before_missing_cloud =
            local.version_messages.load() + local.status_messages.load();
        set_message(agent, {});
        const int missing_cloud_rc = send_cloud(
            agent,
            "studio-serial-1",
            R"({"info":{"command":"get_version","sequence_id":"30006"}})",
            0,
            0
        );
        const bool missing_cloud_did_not_fallback =
            missing_cloud_rc == kBambuConnectFailed &&
            local.version_messages.load() + local.status_messages.load() == local_messages_before_missing_cloud;
        set_message(agent, cloud_callback);

        const int cloud_messages_before_missing_local =
            cloud.version_messages.load() + cloud.status_messages.load();
        set_local_message(agent, {});
        const int missing_local_rc = send_printer(
            agent,
            "studio-serial-2",
            R"({"info":{"command":"get_version","sequence_id":"30007"}})",
            0,
            0
        );
        const bool missing_local_did_not_fallback =
            missing_local_rc == kBambuConnectFailed &&
            cloud.version_messages.load() + cloud.status_messages.load() == cloud_messages_before_missing_local;
        set_local_message(agent, local_callback);
        out.callback_tunnels_exact =
            missing_cloud_did_not_fallback && missing_local_did_not_fallback;

        const int local2_before_replacement = status_count(local, "studio-serial-2");
        const int local3_before_replacement = status_count(local, "studio-serial-3");
        const int replacement_connect_before = local_connect_callbacks.load();
        const std::string selected_before_replacement = get_selected_machine(agent);
        const int replacement_rc = connect_printer(
            agent,
            "studio-serial-3",
            "127.0.0.1",
            "user",
            "pass",
            false
        );
        const int replacement_push_rc = send_printer(
            agent,
            "studio-serial-3",
            R"({"pushing":{"command":"pushall","sequence_id":"replacement","version":1,"push_target":1}})",
            0,
            0
        );
        out.local_replacement_exact =
            replacement_rc == 0 && replacement_push_rc == 0 &&
            local_connect_callbacks.load() == replacement_connect_before + 1 &&
            get_selected_machine(agent) == selected_before_replacement &&
            status_count(local, "studio-serial-3") == local3_before_replacement + 1 &&
            status_count(local, "studio-serial-2") == local2_before_replacement;
        set_selected_machine(agent, selected_before_replacement);

        add_subscribe(agent, {"studio-serial-3"});
        const int same_cloud_before = status_count(cloud, "studio-serial-3");
        const int same_local_before = status_count(local, "studio-serial-3");
        out.same_serial_exact = wait_until([&] {
            return status_count(cloud, "studio-serial-3") > same_cloud_before;
        }) && status_count(local, "studio-serial-3") == same_local_before;

        const int disconnect_local_before = status_count(local, "studio-serial-3");
        const int disconnect_cloud_before = status_count(cloud, "studio-serial-1");
        const int disconnect_rc = disconnect_printer(agent);
        std::this_thread::sleep_for(std::chrono::milliseconds(2'300));
        out.disconnect_exact =
            disconnect_rc == 0 &&
            status_count(cloud, "studio-serial-1") == disconnect_cloud_before &&
            status_count(local, "studio-serial-3") == disconnect_local_before;

        out.cloud_version_messages = cloud.version_messages.load();
        out.cloud_status_messages = cloud.status_messages.load();
        out.local_version_messages = local.version_messages.load();
        out.local_status_messages = local.status_messages.load();
        out.printer_connected_callbacks = printer_connected_callbacks.load();
        out.local_connect_callbacks = local_connect_callbacks.load();
        out.operation_posts = hub_operation_count();
        out.cloud_status_body = status_body(cloud, "studio-serial-1");
        destroy_agent(agent);
        print_json(out);
        return 0;
    }

    if (!failure_mode) {
