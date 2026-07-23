    if (send_cloud(agent, "studio-serial-1", R"({"info":{"command":"get_version","sequence_id":"20001"}})", 0, 0) != 0) {
        fail(agent, destroy_agent, "cloud get_version request failed");
    }
    if (!out.version_callback) {
        fail(agent, destroy_agent, "cloud get_version request did not emit Studio version info");
    }
    if (!failure_mode && send_cloud(agent, "studio-serial-1", R"({"system":{"command":"ledctrl","led_node":"chamber_light","sequence_id":"20002","led_mode":"off","led_on_time":500,"led_off_time":500,"loop_times":1,"interval_time":1000}})", 0, 0) != 0) {
        fail(agent, destroy_agent, "cloud printer light control failed");
    }
    if (!failure_mode && send_cloud(agent, "studio-serial-1", R"({"print":{"command":"set_nozzle_temp","extruder_index":1,"target_temp":245,"sequence_id":"20003"}})", 0, 0) != 0) {
        fail(agent, destroy_agent, "cloud printer nozzle temperature control failed");
    }
    const auto before_start_subscribe_messages = synchronous_message_callbacks.load();
    if (start_subscribe(agent, "app") != kBambuInvalidResult) {
        fail(agent, destroy_agent, "unsupported module subscription control did not return -19");
    }
    if (synchronous_message_callbacks.load() != before_start_subscribe_messages) {
        fail(agent, destroy_agent, "module focus subscription emitted a printer status callback");
    }
    before_messages = message_count.load();
    if (add_subscribe(agent, {"studio-serial-1"}) != 0) {
        fail(agent, destroy_agent, "cloud printer subscription failed");
    }
    const auto add_subscribe_messages = message_count.load() - before_messages;
    out.subscribe_messages = add_subscribe_messages;
    if (!failure_mode && add_subscribe_messages == 0) {
        fail(agent, destroy_agent, "cloud printer subscription did not emit Studio push status");
    }
    before_messages = message_count.load();
    const auto heartbeat_deadline = std::chrono::steady_clock::now() + std::chrono::seconds(3);
    while (message_count.load() == before_messages && std::chrono::steady_clock::now() < heartbeat_deadline) {
        std::this_thread::sleep_for(std::chrono::milliseconds(50));
    }
    out.heartbeat_messages = message_count.load() - before_messages;
    if (!failure_mode && out.heartbeat_messages == 0) {
        fail(agent, destroy_agent, "cloud printer subscription did not keep Studio push status alive");
    }
    if (cloud_printer_connected_callbacks.load() != expected_cloud_connection_callbacks) {
        fail(
            agent,
            destroy_agent,
            "cloud printer connection callback count changed after later status pushes: expected=" +
                std::to_string(expected_cloud_connection_callbacks) + " actual=" +
                std::to_string(cloud_printer_connected_callbacks.load())
        );
    }
    before_messages = message_count.load();
    const auto selected_before_direct_connect = get_selected_machine(agent);
    const auto direct_cloud_callbacks_before = cloud_printer_connected_callbacks.load();
    out.direct_connect_rc = connect_printer(
        agent,
        "studio-serial-1",
        "studio-address-marker.invalid",
        "user",
        "studio-access-marker",
        false
    );
    out.connect_messages = message_count.load() - before_messages;
    out.direct_message_rc = send_printer(agent, "studio-serial-1", "G28 X", 0, 0);
    if (!failure_mode &&
        (out.direct_connect_rc != 0 ||
         out.direct_connect_callback ||
         !out.local_connect_callback ||
         !out.local_connect_transport_redacted ||
         out.connect_messages != 0 ||
         cloud_printer_connected_callbacks.load() != direct_cloud_callbacks_before ||
         get_selected_machine(agent) != selected_before_direct_connect)) {
        fail(agent, destroy_agent, "direct printer connect did not preserve the LAN callback split");
    }
    if (failure_mode) {
        if (out.direct_message_rc == 0) {
            fail(agent, destroy_agent, "direct printer message unexpectedly succeeded in failure mode");
        }
    } else if (out.direct_message_rc != 0) {
        fail(agent, destroy_agent, "direct printer message did not submit operation");
    }

    std::atomic<int> camera_callbacks{0};
    std::atomic<int> golive_callbacks{0};
    std::string golive_camera_url;
    const int camera_rc = get_camera_url(
        agent,
        "studio-serial-1|devver|\"tutk\"",
        [&out, &camera_callbacks](std::string url) {
            ++camera_callbacks;
            out.camera_url = std::move(url);
        }
    );
    const int golive_camera_rc = get_camera_url_for_golive(
        agent,
        "studio-serial-1|devver|\"tutk\"",
        "probe-liveview",
        [&golive_camera_url, &golive_callbacks](std::string url) {
            ++golive_callbacks;
            golive_camera_url = std::move(url);
        }
    );
    out.camera_unavailable_exact =
        camera_rc == kBambuInvalidResult &&
        golive_camera_rc == kBambuInvalidResult &&
        camera_callbacks.load() == 1 &&
        golive_callbacks.load() == 1 &&
        out.camera_url.empty() &&
        golive_camera_url.empty();
    if (!out.camera_unavailable_exact) {
        fail(agent, destroy_agent, "unavailable camera ABI did not return -19 and one empty callback");
    }

    out.ft_abi_version = ft_abi_version();
    FT_TunnelHandle* tunnel = nullptr;
    FT_JobHandle* job = nullptr;
    std::string ft_msg;
    int cb_result_ec = 0;
    ft_job_result job_result{};
    if (out.ft_abi_version != 1) fail(agent, destroy_agent, "unexpected ft ABI version");
    if (ft_tunnel_create("ft://probe", &tunnel) != kFtOk || !tunnel) fail(agent, destroy_agent, "ft tunnel create failed");
    out.ft_start_connect_rc = ft_tunnel_start_connect(tunnel, ft_connect_cb, &ft_msg);
    if (out.ft_start_connect_rc != kFtOk || !contains(ft_msg, "unsupported_file_transfer")) {
        fail(agent, destroy_agent, "ft start connect did not return unsupported callback");
    }
    out.ft_sync_rc = ft_tunnel_sync_connect(tunnel);
    if (out.ft_sync_rc != kFtEio) fail(agent, destroy_agent, "ft sync did not return FT_EIO");
    if (ft_job_create(R"({"op":"probe"})", &job) != kFtOk || !job) fail(agent, destroy_agent, "ft job create failed");
    if (ft_job_set_result_cb(job, ft_result_cb, &cb_result_ec) != kFtOk) fail(agent, destroy_agent, "ft result callback registration failed");
    out.ft_start_job_rc = ft_tunnel_start_job(tunnel, job);
    if (out.ft_start_job_rc != kFtOk) fail(agent, destroy_agent, "ft start job failed");
    if (ft_job_get_result(job, 1000, &job_result) != kFtOk || job_result.ec != kFtEio || cb_result_ec != kFtEio) {
        fail(agent, destroy_agent, "ft job result did not report FT_EIO");
    }
    out.ft_job_result_ec = job_result.ec;
    out.ft_cancel_rc = ft_job_cancel(job);
    if (out.ft_cancel_rc != kFtOk) fail(agent, destroy_agent, "ft cancel failed");
    if (ft_tunnel_shutdown(tunnel) != kFtOk) fail(agent, destroy_agent, "ft shutdown failed");
    ft_job_release(job);
    ft_tunnel_release(tunnel);

    if (!failure_mode) {
        if (set_local_message(agent, [&local_message_count](std::string dev_id, std::string body) {
            if (dev_id == "studio-serial-1" && contains(body, R"("command":"push_status")")) {
                ++local_message_count;
            }
        }) != 0) {
            fail(agent, destroy_agent, "local message callback registration failed");
        }
        const auto cloud_messages_before_pushall = message_count.load();
        const auto local_messages_before_pushall = local_message_count.load();
        if (send_cloud(agent, "studio-serial-1", R"({"pushing":{"command":"pushall","sequence_id":"20004","version":1,"push_target":1}})", 0, 0) != 0) {
            fail(agent, destroy_agent, "local-preferred pushall request failed");
        }
        if (message_count.load() == cloud_messages_before_pushall) {
            fail(agent, destroy_agent, "push status did not use the registered cloud message callback");
        }
        if (local_message_count.load() != local_messages_before_pushall) {
            fail(agent, destroy_agent, "push status duplicated one Hub update through the local callback");
        }
    }

    if (user_logout(agent, false) != 0) fail(agent, destroy_agent, "logout failed");
    if (!get_selected_machine(agent).empty()) {
        fail(agent, destroy_agent, "logout retained the previous account printer selection");
    }
    out.logout_command = build_logout_cmd(agent);
    if (!contains(out.logout_command, "studio_useroffline")) {
        fail(agent, destroy_agent, "logout envelope lacked studio_useroffline");
    }

    if (!failure_mode && !stale_token_mode) {
        void* lan_agent = create_agent("");
        if (!lan_agent) fail(agent, destroy_agent, "LAN-only agent creation failed");
        std::atomic<int> lan_tunnel_callbacks{0};
        std::atomic<int> lan_direct_callbacks{0};
        std::atomic<int> lan_local_callbacks{0};
        const auto lan_config_dir = (std::filesystem::path(config_dir) / "lan-only").string();
        if (set_config_dir(lan_agent, lan_config_dir) != 0 ||
            set_printer_connected(lan_agent, [&lan_tunnel_callbacks, &lan_direct_callbacks](std::string dev_id) {
            if (dev_id == "tunnel/lan-only") ++lan_tunnel_callbacks;
            if (dev_id == "lan-only") ++lan_direct_callbacks;
        }) != 0 ||
            set_local_connect(lan_agent, [&lan_local_callbacks](int status, std::string dev_id, std::string) {
                if (status == 0 && dev_id == "lan-only") ++lan_local_callbacks;
            }) != 0 ||
            connect_printer(lan_agent, "lan-only", "127.0.0.1", "user", "pass", false) !=
                kBambuConnectFailed) {
            destroy_agent(lan_agent);
            fail(agent, destroy_agent, "LAN-only printer bypassed Hub printer eligibility");
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(2500));
        if (lan_direct_callbacks.load() != 0 ||
            lan_tunnel_callbacks.load() != 0 ||
            lan_local_callbacks.load() != 0) {
            destroy_agent(lan_agent);
            fail(agent, destroy_agent, "LAN-only printer was reclassified as a cloud tunnel");
        }
        destroy_agent(lan_agent);
    }
    destroy_agent(agent);
    print_json(out);
    return 0;
}
