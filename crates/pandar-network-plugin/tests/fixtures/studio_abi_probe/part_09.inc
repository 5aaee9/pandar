        void* restored_agent = create_agent("probe-log-restored");
        if (!restored_agent) fail(agent, destroy_agent, "restored agent creation failed");
        if (set_config_dir(restored_agent, config_dir) != 0) {
            destroy_agent(restored_agent);
            fail(agent, destroy_agent, "restored set config dir failed");
        }
        out.restored_login = is_user_login(restored_agent);
        if (!out.restored_login || !contains(build_login_cmd(restored_agent), "probe-token")) {
            destroy_agent(restored_agent);
            fail(agent, destroy_agent, "restored agent did not reuse persisted login");
        }
        unsigned int restored_http_code = 0;
        std::string restored_body;
        if (get_print_info(restored_agent, &restored_http_code, &restored_body) != 0 || restored_http_code != 200) {
            destroy_agent(restored_agent);
            fail(agent, destroy_agent, "restored agent could not fetch printer list");
        }
        destroy_agent(restored_agent);
    }

    BBL::TaskQueryParams query;
    query.dev_id = "studio-serial-1";
    std::string tasks_body;
    if (!failure_mode) {
        out.tasks_rc = get_tasks(agent, query, &tasks_body);
        if (out.tasks_rc != 0) fail(agent, destroy_agent, "task listing failed");
        if (!contains(tasks_body, R"("total":0)") || !contains(tasks_body, R"("hits":[])")) {
            fail(agent, destroy_agent, "task listing did not return a Studio-compatible empty page");
        }
    } else {
        out.tasks_rc = -1;
    }

    BBL::PrintParams params{};
    params.dev_id = "studio-serial-1";
    params.task_name = "probe.3mf";
    params.project_name = "wrong-display-name.3mf";
    params.filename = argv[2];
    params.plate_index = 1;
    params.connection_type = "cloud";
    params.print_type = "from_normal";
    params.task_bed_type = "textured_plate";
    params.task_use_ams = true;
    params.task_bed_leveling = true;
    params.auto_bed_leveling = 2;
    params.task_flow_cali = false;
    params.auto_flow_cali = 1;
    params.auto_offset_cali = 0;
    params.task_record_timelapse = false;

    BBL::OnUpdateStatusFn update = [&out](int stage, int code, std::string body) {
        out.update_stage = stage;
        out.update_code = code;
        out.update_body = std::move(body);
    };
    BBL::WasCancelledFn cancelled = [] { return false; };
    BBL::OnWaitFn wait = [](int, std::string) { return true; };
    out.print_rc = start_print(agent, params, update, cancelled, wait);
    if (failure_mode) {
        if (out.print_rc == 0 || !contains(out.update_body, "plugin_forbidden")) {
            fail(agent, destroy_agent, "print failure did not map to plugin_forbidden");
        }
        assert_redacted_stable_error(
            agent,
            destroy_agent,
            out.update_body,
            "plugin_forbidden",
            {"secret", "raw-forbidden-message", "\"ticket\"", "\"token\"", "\"path\"", "/tmp/secret.3mf"}
        );
    } else if (out.print_rc != 0) {
        fail(agent, destroy_agent, "print submission failed");
    } else if (out.update_stage != 6 || out.update_code != 0 || out.update_body != "3") {
        fail(agent, destroy_agent, "print success callback did not match Studio stage contract");
    }
    if (failure_mode) {
        destroy_agent(agent);
        print_json(out);
        return 0;
    }

    BBL::OnUpdateStatusFn sdcard_update = [&out](int, int, std::string body) {
        out.sdcard_update_body = std::move(body);
    };
    int sdcard_rc = start_sdcard_print(agent, params, sdcard_update, cancelled, wait);
    if (sdcard_rc == 0 || !contains(out.sdcard_update_body, "unsupported_file_transfer")) {
        fail(agent, destroy_agent, "SD-card print did not return stable unsupported callback");
    }

    const auto abi_thread_id = std::this_thread::get_id();
    std::atomic<int> synchronous_printer_connected_callbacks{0};
    std::atomic<int> cloud_printer_connected_callbacks{0};
    std::atomic<int> server_connected_callbacks{0};
    if (set_server_connected(agent, [&server_connected_callbacks](int status, int reason) {
        if (status == 0 && reason == 0) ++server_connected_callbacks;
    }) != 0) {
        fail(agent, destroy_agent, "server connected callback registration failed");
    }
    const int expected_server_connected_callbacks = failure_mode ? 1 : 0;
    if (connect_server(agent) != 0 ||
        server_connected_callbacks.load() != expected_server_connected_callbacks) {
        fail(agent, destroy_agent, "server connect replayed an already observed connection transition");
    }
    if (connect_server(agent) != 0 ||
        server_connected_callbacks.load() != expected_server_connected_callbacks) {
        fail(agent, destroy_agent, "server connect repeated the unchanged connection callback");
    }
    if (refresh_connection(agent) != 0 ||
        server_connected_callbacks.load() != expected_server_connected_callbacks) {
        fail(agent, destroy_agent, "connection refresh repeated the unchanged server connection callback");
    }
    if (!is_server_connected(agent) || send_printer(agent, "studio-serial-1", "M999", 0, 0) == 0) {
        fail(agent, destroy_agent, "invalid printer operation precondition changed");
    }
    if (!is_server_connected(agent)) {
        fail(agent, destroy_agent, "business operation error changed proven Hub reachability");
    }
    if (refresh_connection(agent) != 0 ||
        server_connected_callbacks.load() != expected_server_connected_callbacks ||
        !is_server_connected(agent)) {
        fail(agent, destroy_agent, "healthy connection refresh changed after a business operation error");
    }
    if (refresh_connection(agent) != 0 ||
        server_connected_callbacks.load() != expected_server_connected_callbacks) {
        fail(agent, destroy_agent, "healthy connection refresh repeated the unchanged callback");
    }
    if (update_cert(agent) != kBambuInvalidResult) {
        fail(agent, destroy_agent, "unsupported certificate update did not return -19");
    }
    std::atomic<int> synchronous_local_connect_callbacks{0};
    if (set_local_connect(agent, [&out, &synchronous_local_connect_callbacks, abi_thread_id](int status, std::string dev_id, std::string body) {
        if (status == 0 && dev_id == "studio-serial-1" && contains(body, R"("dev_type":"N6")")) {
            out.local_connect_callback = true;
            out.local_connect_transport_redacted =
                contains(body, R"("dev_ip":"")") &&
                !contains(body, "192.0.2.10") &&
                !contains(body, "studio-address-marker.invalid") &&
                !contains(body, "studio-access-marker");
            if (std::this_thread::get_id() == abi_thread_id) {
                ++synchronous_local_connect_callbacks;
            }
        }
    }) != 0) {
        fail(agent, destroy_agent, "local connect callback registration failed");
    }
    std::atomic<int> message_count{0};
    std::atomic<int> local_message_count{0};
    std::atomic<int> synchronous_message_callbacks{0};
    if (set_message(agent, [&out, &message_count, &synchronous_message_callbacks, abi_thread_id, agent, destroy_agent, failure_mode](std::string dev_id, std::string body) {
        if (std::this_thread::get_id() == abi_thread_id) ++synchronous_message_callbacks;
        if (dev_id == "studio-serial-1" && contains(body, R"("command":"get_version")")) {
            if (failure_mode) {
                out.version_callback = true;
                return;
            }
            if (!contains(body, R"("sequence_id":"20001")") ||
                !contains(body, R"("module")") ||
                !contains(body, R"("name":"ota")") ||
                !contains(body, R"("product_name":"N6")")) {
                fail(agent, destroy_agent, "Studio get_version response did not include version modules");
            }
            out.version_callback = true;
            return;
        }
        if (dev_id != "studio-serial-1" || !contains(body, R"("command":"push_status")")) return;
        if (failure_mode) {
            out.message_callback = true;
            ++message_count;
            return;
        }
        if (!contains(body, R"("ipcam")")) return;
        if (contains(body, R"("mc_percent":36)")) {
            ++message_count;
            return;
        }
        if (contains(body, R"("gcode_state":"IDLE")") ||
            contains(body, R"("mc_percent":0)") ||
            contains(body, R"("mc_remaining_time":0)") ||
            contains(body, "100%") ||
            !contains(body, R"("sdcard":false)") ||
            !contains(body, R"("ipcam_dev":"0")") ||
            !contains(body, R"("liveview":{"local":"none","remote":"none"})") ||
            !contains(body, R"("rtsp_url":"")") ||
            !contains(body, R"("support_mqtt_alive":true)") ||
            !contains(body, R"("gcode_state":"RUNNING")") ||
            !contains(body, R"("mc_percent":37)") ||
            !contains(body, R"("mc_remaining_time":52)") ||
            !contains(body, R"("layer_num":12)") ||
            !contains(body, R"("total_layer_num":120)") ||
            !contains(body, R"("task_id":"task-42")") ||
            !contains(body, R"("project_id":"0")") ||
            !contains(body, R"("profile_id":"0")") ||
            !contains(body, R"("subtask_id":"subtask-42")") ||
            !contains(body, R"("gcode_file":"drawer-organizer.gcode")") ||
            !contains(body, R"("subtask_name":"drawer-organizer")") ||
            !contains(body, R"("hms":[{"attr":134152704,"code":32785}])") ||
            !contains(body, R"("nozzle_temper":27)") ||
            !contains(body, R"("nozzle_target_temper":215)") ||
            !contains(body, R"("nozzle_temper2":28)") ||
            !contains(body, R"("nozzle_target_temper2":220)") ||
            !contains(body, R"("nozzle_type":"XS01")") ||
            !contains(body, R"("nozzle_diameter":0.4)") ||
            !contains(body, R"("nozzle":{"exist":3)") ||
            !contains(body, R"("id":1,"diameter":0.4,"type":"XH05")") ||
            !contains(body, R"("id":0,"diameter":0.4,"type":"XS01")") ||
            !contains(body, R"("bed_temper":60)") ||
            !contains(body, R"("bed_target_temper":65)") ||
            !contains(body, R"("chamber_temper":32)") ||
            !contains(body, R"("printer_type":"N6")") ||
            !contains(body, R"("support_chamber":false)") ||
            !contains(body, R"("support_chamber_temp_display":false)") ||
            !contains(body, R"("cfg":"8000000000000001")") ||
            !contains(body, R"("fun":"8000004100000020")") ||
            !contains(body, R"("aux":"A4003001")") ||
            !contains(body, R"("stat":"1000000001")") ||
            !contains(body, R"("device")") ||
            !contains(body, R"("type":1)") ||
            !contains(body, R"("extruder")") ||
            !contains(body, R"("state":18)") ||
            !contains(body, R"({"id":0,"filam_bak":[],"info":8,"temp":14090267,"spre":65535,"snow":65535,"star":65535,"stat":0,"hnow":0},{"id":1,"filam_bak":[],"info":8,"temp":14417948,"spre":65535,"snow":65535,"star":65535,"stat":0,"hnow":1})") ||
            !contains(body, R"("ams_exist_bits":"3")") ||
            !contains(body, R"("tray_exist_bits":"ff")") ||
            !contains(body, R"("tray_now":"3")") ||
            !contains(body, R"("id":"0","info":"00000E00")") ||
            !contains(body, R"("id":"1","info":"01000E00")") ||
            !contains(body, R"("humidity":"3")") ||
            !contains(body, R"("humidity_raw":"25")") ||
            !contains(body, R"("temp":"28.5")") ||
            !contains(body, R"("temp":"28.1")") ||
            !contains(body, R"("tray_info_idx":"GFG50")") ||
            !contains(body, R"("tray_type":"PETG-CF")") ||
            !contains(body, R"("tray_type":"PLA")") ||
            !contains(body, R"("tray_info_idx":"GFL99")") ||
            !contains(body, R"("tray_info_idx":"GFA00")") ||
            !contains(body, R"("tray_info_idx":"GFG00")") ||
            !contains(body, R"("tray_type":"ABS")") ||
            !contains(body, R"("tray_info_idx":"GFB00")") ||
            !contains(body, R"("tray_type":"PLA-CF")") ||
            !contains(body, R"("tray_info_idx":"GFA50")") ||
            !contains(body, R"("vir_slot":[)") ||
            !contains(body, R"("id":"254")") ||
            !contains(body, R"("id":"255")") ||
            !contains(body, R"("tray_type":"PETG")") ||
            !contains(body, R"("lights_report":[{"node":"chamber_light","mode":"on"}])")) {
            fail(agent, destroy_agent, "Studio push status did not include plugin printer telemetry: " + body);
        }
        out.message_callback = true;
        ++message_count;
    }) != 0) {
        fail(agent, destroy_agent, "message callback registration failed");
    }
    if (set_printer_connected(agent, [&out, &cloud_printer_connected_callbacks, &synchronous_printer_connected_callbacks, abi_thread_id](std::string dev_id) {
        if (dev_id == "tunnel/studio-serial-1") {
            ++cloud_printer_connected_callbacks;
        }
        if (dev_id == "studio-serial-1") {
            out.direct_connect_callback = true;
            if (std::this_thread::get_id() == abi_thread_id) {
                ++synchronous_printer_connected_callbacks;
            }
        }
    }) != 0) {
        fail(agent, destroy_agent, "printer connected callback registration failed");
    }

    const auto before_selected_messages = synchronous_message_callbacks.load();
    const auto before_selected_connections = synchronous_printer_connected_callbacks.load();
    const auto before_selected_local_connections = synchronous_local_connect_callbacks.load();
    if (set_selected_machine(agent, "studio-selected-only") != 0 ||
        get_selected_machine(agent) != "studio-selected-only" ||
        set_selected_machine(agent, "studio-serial-1") != 0 ||
        get_selected_machine(agent) != "studio-serial-1") {
        fail(agent, destroy_agent, "selected cloud printer failed");
    }
    out.selected_machine_messages = synchronous_message_callbacks.load() - before_selected_messages;
    if (out.selected_machine_messages != 0 ||
        synchronous_printer_connected_callbacks.load() != before_selected_connections ||
        synchronous_local_connect_callbacks.load() != before_selected_local_connections) {
        fail(agent, destroy_agent, "selected cloud printer emitted connection or status callbacks");
    }

    int before_messages = message_count.load();
    if (!failure_mode &&
        (send_cloud(agent, "studio-serial-1", R"({"pushing":{"command":"pushall","sequence_id":"20000","version":1,"push_target":1}})", 0, 0) != 0 ||
         cloud_printer_connected_callbacks.load() != 1 || message_count.load() == before_messages)) {
        fail(agent, destroy_agent, "selected cloud status request did not reach Studio");
    }
    if (set_selected_machine(agent, "") != 0) {
        fail(agent, destroy_agent, "selected cloud status target could not be cleared");
    }
    before_messages = message_count.load();
    if (send_cloud(agent, "studio-serial-1", R"({"pushing":{"command":"pushall","sequence_id":"20000-untargeted","version":1,"push_target":1}})", 0, 0) !=
            kBambuConnectFailed ||
        cloud_printer_connected_callbacks.load() != (failure_mode ? 0 : 1) ||
        message_count.load() != before_messages) {
        fail(agent, destroy_agent, "unselected and unsubscribed status request reported false success");
    }
    before_messages = message_count.load();
    const int subscribed_connection_callbacks = failure_mode ? 1 : 2;
    if (add_subscribe(agent, {"studio-serial-1"}) != 0 ||
        cloud_printer_connected_callbacks.load() != subscribed_connection_callbacks ||
        message_count.load() == before_messages ||
        set_selected_machine(agent, "studio-serial-1") != 0) {
        fail(agent, destroy_agent, "explicit cloud subscription did not emit first status");
    }
    if (!failure_mode && !wait_until([&] { return out.message_callback; })) {
        fail(agent, destroy_agent, "explicit subscription did not refresh Studio status");
    }
    const int expected_cloud_connection_callbacks = cloud_printer_connected_callbacks.load();
