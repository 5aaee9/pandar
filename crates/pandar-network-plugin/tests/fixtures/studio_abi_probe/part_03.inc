int main(int argc, char** argv) {
    if (argc < 5) {
        std::cerr << "usage: studio_abi_probe <plugin-library> <artifact-path> <mode> <config-dir>\n";
        return 2;
    }
    const std::string mode = argv[3];
    const bool failure_mode = mode == "failure";
    const bool stale_token_mode = mode == "stale-token-refresh";
    const bool native_print_error_mode = mode == "native-print-error";
    const bool axis_features_mode = mode == "axis-features";
    const bool connection_readiness_mode = mode == "connection-readiness";
    const bool background_timeout_mode = mode == "background-timeout";
    const bool auth_rejected_mode = mode == "auth-rejected";
    const bool printer_presence_mode = mode == "printer-presence";
    const bool account_transition_mode = mode == "account-transition";
    const bool account_exchange_race_mode = mode == "account-exchange-race";
    const bool token_rotation_mode = mode == "token-rotation";
    const bool token_rotation_offline_mode = mode == "token-rotation-offline";
    const bool token_refresh_mode = token_rotation_mode || token_rotation_offline_mode;
    const bool freshness_claim_mode = mode == "freshness-claim";
    const bool firmware_claim_race_mode = mode == "firmware-claim-race";
    const bool callback_order_mode = mode == "callback-order";
    const bool request_admission_mode = mode == "request-admission";
    const bool camera_available_mode = mode == "camera-available";
    const bool camera_unavailable_mode = mode == "camera-unavailable";
    const bool no_auth_recovery_mode = mode == "no-auth-recovery";
    const bool official_no_auth_recovery_mode = mode == "official-no-auth-recovery";
    const bool official_no_auth_logout_recovery_mode =
        mode == "official-no-auth-logout-recovery";
    const bool official_no_auth_lifecycle_mode =
        official_no_auth_recovery_mode || official_no_auth_logout_recovery_mode;
    if (!failure_mode && !stale_token_mode && !native_print_error_mode && !axis_features_mode &&
        !connection_readiness_mode && !background_timeout_mode && !auth_rejected_mode &&
        !printer_presence_mode && !account_transition_mode && !account_exchange_race_mode &&
        !token_refresh_mode &&
        !freshness_claim_mode && !firmware_claim_race_mode && !callback_order_mode &&
        !request_admission_mode && !camera_available_mode &&
        !camera_unavailable_mode && !no_auth_recovery_mode &&
        !official_no_auth_lifecycle_mode &&
        mode != "success") {
        std::cerr << "unsupported probe mode\n";
        return 2;
    }

    Library lib(argv[1], official_no_auth_lifecycle_mode);
    if (!lib.ok()) {
        std::cerr << "failed to load plugin library\n";
        return 3;
    }

#include "function_types.inc"

    auto create_agent = lib.sym<create_agent_fn>("bambu_network_create_agent");
    auto destroy_agent = lib.sym<destroy_agent_fn>("bambu_network_destroy_agent");
    auto start = lib.sym<start_fn>("bambu_network_start");
    auto set_config_dir = lib.sym<set_config_dir_fn>("bambu_network_set_config_dir");
    auto init_log = lib.sym<start_fn>("bambu_network_init_log");
    auto set_cert_file = lib.sym<set_cert_file_fn>("bambu_network_set_cert_file");
    auto set_country_code = lib.sym<set_country_code_fn>("bambu_network_set_country_code");
    auto set_extra_http_header =
        lib.sym<set_extra_http_header_fn>("bambu_network_set_extra_http_header");
    auto get_host = lib.sym<string_agent_fn>("bambu_network_get_bambulab_host");
    auto get_token = lib.sym<token_fn>("bambu_network_get_my_token");
    auto get_profile = lib.sym<token_fn>("bambu_network_get_my_profile");
    auto change_user = lib.sym<change_user_fn>("bambu_network_change_user");
    auto set_user_login =
        lib.sym<set_user_login_fn>("bambu_network_set_on_user_login_fn");
    auto is_user_login = lib.sym<is_user_login_fn>("bambu_network_is_user_login");
    auto build_login_cmd = lib.sym<string_agent_fn>("bambu_network_build_login_cmd");
    auto build_login_info = lib.sym<string_agent_fn>("bambu_network_build_login_info");
    auto get_user_id = lib.sym<string_agent_fn>("bambu_network_get_user_id");
    auto get_print_info = lib.sym<print_info_fn>("bambu_network_get_user_print_info");
    auto get_printer_firmware =
        lib.sym<printer_firmware_fn>("bambu_network_get_printer_firmware");
    auto get_selected_machine = lib.sym<string_agent_fn>("bambu_network_get_user_selected_machine");
    auto get_tasks = lib.sym<tasks_fn>("bambu_network_get_user_tasks");
    auto start_print = lib.sym<start_print_fn>("bambu_network_start_print");
    auto start_sdcard_print = lib.sym<start_sdcard_fn>("bambu_network_start_send_gcode_to_sdcard");
    auto connect_printer = lib.sym<connect_printer_fn>("bambu_network_connect_printer");
    auto disconnect_printer = lib.sym<disconnect_printer_fn>("bambu_network_disconnect_printer");
    auto send_cloud = lib.sym<send_cloud_fn>("bambu_network_send_message");
    auto send_printer = lib.sym<send_printer_fn>("bambu_network_send_message_to_printer");
    auto set_selected_machine = lib.sym<selected_machine_fn>("bambu_network_set_user_selected_machine");
    auto start_subscribe = lib.sym<start_subscribe_fn>("bambu_network_start_subscribe");
    auto add_subscribe = lib.sym<add_subscribe_fn>("bambu_network_add_subscribe");
    auto del_subscribe = lib.sym<del_subscribe_fn>("bambu_network_del_subscribe");
    auto set_printer_connected = lib.sym<set_printer_connected_fn>("bambu_network_set_on_printer_connected_fn");
    auto set_server_connected = lib.sym<set_server_connected_fn>("bambu_network_set_on_server_connected_fn");
    auto set_http_error = lib.sym<set_http_error_fn>("bambu_network_set_on_http_error_fn");
    auto set_local_connect = lib.sym<set_local_connect_fn>("bambu_network_set_on_local_connect_fn");
    auto set_message = lib.sym<set_message_fn>("bambu_network_set_on_message_fn");
    auto set_local_message = lib.sym<set_message_fn>("bambu_network_set_on_local_message_fn");
    auto set_user_message = lib.sym<set_message_fn>("bambu_network_set_on_user_message_fn");
    auto set_country_callback =
        lib.sym<set_country_callback_fn>("bambu_network_set_get_country_code_fn");
    auto set_subscribe_failure = lib.sym<set_subscribe_failure_fn>(
        "bambu_network_set_on_subscribe_failure_fn"
    );
    auto set_queue_on_main =
        lib.sym<set_queue_on_main_fn>("bambu_network_set_queue_on_main_fn");
    auto set_server_error =
        lib.sym<set_server_error_fn>("bambu_network_set_server_callback");
    auto connect_server = lib.sym<connect_server_fn>("bambu_network_connect_server");
    auto is_server_connected = lib.sym<is_server_connected_fn>("bambu_network_is_server_connected");
    auto refresh_connection = lib.sym<refresh_connection_fn>("bambu_network_refresh_connection");
    auto update_cert = lib.sym<update_cert_fn>("bambu_network_update_cert");
    auto get_camera_url = lib.sym<camera_url_fn>("bambu_network_get_camera_url");
    auto get_camera_url_for_golive =
        lib.sym<camera_golive_url_fn>("bambu_network_get_camera_url_for_golive");
    auto user_logout = lib.sym<logout_fn>("bambu_network_user_logout");
    auto build_logout_cmd = lib.sym<string_agent_fn>("bambu_network_build_logout_cmd");
    auto parse_operation =
        lib.sym<parse_operation_fn>("pandar_plugin_operation_json_from_gcode");
    auto free_with_capacity =
        lib.sym<free_with_capacity_fn>("pandar_plugin_free_with_capacity");

    auto ft_abi_version = lib.sym<int (*)()>("ft_abi_version");
    auto ft_tunnel_create = lib.sym<int (*)(const char*, FT_TunnelHandle**)>("ft_tunnel_create");
    auto ft_tunnel_start_connect = lib.sym<int (*)(FT_TunnelHandle*, ft_tunnel_connect_cb, void*)>("ft_tunnel_start_connect");
    auto ft_tunnel_sync_connect = lib.sym<int (*)(FT_TunnelHandle*)>("ft_tunnel_sync_connect");
    auto ft_tunnel_shutdown = lib.sym<int (*)(FT_TunnelHandle*)>("ft_tunnel_shutdown");
    auto ft_tunnel_release = lib.sym<void (*)(FT_TunnelHandle*)>("ft_tunnel_release");
    auto ft_job_create = lib.sym<int (*)(const char*, FT_JobHandle**)>("ft_job_create");
    auto ft_job_set_result_cb = lib.sym<int (*)(FT_JobHandle*, ft_job_result_cb, void*)>("ft_job_set_result_cb");
    auto ft_tunnel_start_job = lib.sym<int (*)(FT_TunnelHandle*, FT_JobHandle*)>("ft_tunnel_start_job");
    auto ft_job_get_result = lib.sym<int (*)(FT_JobHandle*, uint32_t, ft_job_result*)>("ft_job_get_result");
    auto ft_job_cancel = lib.sym<int (*)(FT_JobHandle*)>("ft_job_cancel");
    auto ft_job_release = lib.sym<void (*)(FT_JobHandle*)>("ft_job_release");

    ProbeResult out;
    void* agent = create_agent("probe-log");
    if (!agent) fail(agent, destroy_agent, "agent creation failed");
    const std::string config_dir = argv[4];
    if (stale_token_mode || token_refresh_mode) write_stale_login_state(config_dir);
    if (set_config_dir(agent, config_dir) != 0) fail(agent, destroy_agent, "set config dir failed");

    if (camera_available_mode) {
        const std::string profile = R"({"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"})";
        if (change_user(agent, profile) != 0) {
            fail(agent, destroy_agent, "camera available profile setup failed");
        }
        std::string printers_body;
        unsigned int printers_code = 0;
        if (get_print_info(agent, &printers_code, &printers_body) != 0 || printers_code != 200) {
            fail(agent, destroy_agent, "camera available printer seed failed");
        }
        const std::vector<std::string> camera_devices = {
            "studio-camera-a1",
            "studio-camera-a1-mini",
            "studio-camera-p1s",
            "studio-camera-a2l",
        };
        std::atomic<int> camera_callbacks{0};
        std::atomic<int> golive_callbacks{0};
        std::vector<std::string> camera_urls;
        for (const auto& dev_id : camera_devices) {
            std::string camera_url;
            std::string golive_camera_url;
            const auto request_dev_id = dev_id + "|devver|\"tutk\"";
            const int camera_rc = get_camera_url(
                agent,
                request_dev_id,
                [&camera_url, &camera_callbacks](std::string url) {
                    ++camera_callbacks;
                    camera_url = std::move(url);
                }
            );
            const int golive_camera_rc = get_camera_url_for_golive(
                agent,
                request_dev_id,
                "probe-liveview",
                [&golive_camera_url, &golive_callbacks](std::string url) {
                    ++golive_callbacks;
                    golive_camera_url = std::move(url);
                }
            );
            if (camera_rc != 0 || golive_camera_rc != 0 ||
                !exact_camera_relay_url(camera_url) ||
                !exact_camera_relay_url(golive_camera_url)) {
                fail(agent, destroy_agent, "available camera ABI did not return exact loopback URLs");
            }
            camera_urls.push_back(std::move(camera_url));
            camera_urls.push_back(std::move(golive_camera_url));
        }
        out.camera_url = camera_urls.front();
        out.camera_callback_count = camera_callbacks.load();
        out.camera_golive_callback_count = golive_callbacks.load();
        out.camera_urls_unique = all_unique(camera_urls);
        out.camera_credentials_redacted = true;
        const std::vector<std::string> forbidden = {
            "probe-token",
            "studio-camera-",
            "printer-camera-",
            "probe-user",
        };
        for (const auto& url : camera_urls) {
            for (const auto& value : forbidden) {
                if (contains(url, value)) out.camera_credentials_redacted = false;
            }
        }
        out.camera_available_exact =
            out.camera_callback_count == 4 &&
            out.camera_golive_callback_count == 4 &&
            out.camera_urls_unique &&
            out.camera_credentials_redacted;
        if (!out.camera_available_exact) {
            fail(agent, destroy_agent, "available camera ABI callbacks or redaction were not exact");
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }

    if (camera_unavailable_mode) {
        const std::string profile = R"({"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"})";
        if (change_user(agent, profile) != 0) {
            fail(agent, destroy_agent, "camera unavailable profile setup failed");
        }
        std::string printers_body;
        unsigned int printers_code = 0;
        if (get_print_info(agent, &printers_code, &printers_body) != 0 || printers_code != 200) {
            fail(agent, destroy_agent, "camera unavailable printer seed failed");
        }
        std::atomic<int> camera_callbacks{0};
        std::atomic<int> golive_callbacks{0};
        std::string camera_url;
        std::string golive_camera_url;
        const int camera_rc = get_camera_url(
            agent,
            "studio-serial-1|devver|\"tutk\"",
            [&camera_url, &camera_callbacks](std::string url) {
                ++camera_callbacks;
                camera_url = std::move(url);
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
            camera_url.empty() &&
            golive_camera_url.empty();
        if (!out.camera_unavailable_exact) {
            fail(agent, destroy_agent, "credentials escaped agent-local camera policy");
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }

    if (connection_readiness_mode) {
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
        if (connect_server(agent) != kBambuConnectFailed || is_server_connected(agent) ||
            callbacks.load() != 1 || successful_callbacks.load() != 0) {
            fail(agent, destroy_agent, "not-ready Hub was treated as connected");
        }
        if (refresh_connection(agent) != kBambuConnectFailed || is_server_connected(agent) ||
            callbacks.load() != 1) {
            fail(agent, destroy_agent, "unchanged not-ready Hub repeated the callback");
        }
        if (refresh_connection(agent) != 0 || !is_server_connected(agent) ||
            callbacks.load() != 2 || successful_callbacks.load() != 1) {
            fail(agent, destroy_agent, "ready Hub did not recover connectivity");
        }
        if (refresh_connection(agent) != 0 || !is_server_connected(agent) ||
            callbacks.load() != 2) {
            fail(agent, destroy_agent, "unchanged ready Hub repeated the callback");
        }
        if (refresh_connection(agent) != kBambuConnectFailed || is_server_connected(agent) ||
            callbacks.load() != 3) {
            fail(agent, destroy_agent, "invalid ready payload did not disconnect the Hub");
        }
        if (refresh_connection(agent) != kBambuConnectFailed || is_server_connected(agent) ||
            callbacks.load() != 3) {
            fail(agent, destroy_agent, "readiness loss did not disconnect the Hub");
        }
        if (refresh_connection(agent) != 0 || !is_server_connected(agent) ||
            callbacks.load() != 4 || successful_callbacks.load() != 2) {
            fail(agent, destroy_agent, "second ready observation did not recover connectivity");
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
        std::string printers_body;
        unsigned int printers_code = 0;
        if (get_print_info(agent, &printers_code, &printers_body) != 0 || printers_code != 200) {
            fail(agent, destroy_agent, "background timeout printer seed failed");
        }
        TunnelCapture cloud;
        std::atomic<int> connected_callbacks{0};
        std::atomic<int> disconnected_callbacks{0};
        if (set_message(agent, [&cloud](std::string dev_id, std::string body) {
                capture_tunnel_message(cloud, dev_id, body);
            }) != 0 ||
            set_server_connected(agent, [&connected_callbacks, &disconnected_callbacks](int status, int reason) {
                if (reason != 0) return;
                if (status == 0) ++connected_callbacks;
                else ++disconnected_callbacks;
            }) != 0) {
            fail(agent, destroy_agent, "background timeout callback registration failed");
        }
        if (connect_server(agent) != 0 || !is_server_connected(agent) ||
            connected_callbacks.load() != 0) {
            fail(agent, destroy_agent, "background timeout readiness setup failed");
        }
        add_subscribe(agent, {"studio-serial-1"});
        const int statuses_before_timeout = status_count(cloud, "studio-serial-1");
        if (!wait_until([&] { return disconnected_callbacks.load() == 1; }) ||
            is_server_connected(agent)) {
            fail(agent, destroy_agent, "background printer timeout did not invalidate Hub reachability");
        }
        if (status_count(cloud, "studio-serial-1") != statuses_before_timeout) {
            fail(agent, destroy_agent, "background printer timeout replayed stale push_status");
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
            connected_callbacks.load() != 1 || local_connected_callbacks.load() != 1 ||
            status_count(cloud, "studio-serial-1") != 1) {
            fail(agent, destroy_agent, "freshness claim active subscription setup failed");
        }

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
        const auto version_entered =
            std::filesystem::path(config_dir) / "freshness-version-entered";
        if (!wait_until([&] { return std::filesystem::exists(version_entered); })) {
            version_request.join();
            fail(agent, destroy_agent, "freshness claim version request did not enter Hub");
        }
        int refresh_rc = -1;
        std::thread refresh_request([&] {
            unsigned int code = 0;
            std::string body;
            refresh_rc = get_print_info(agent, &code, &body);
        });
        const auto refresh_entered =
            std::filesystem::path(config_dir) / "freshness-printer-entered";
        if (!wait_until([&] { return std::filesystem::exists(refresh_entered); })) {
            refresh_request.join();
            version_request.join();
            fail(agent, destroy_agent, "freshness claim printer refresh did not enter Hub");
        }
        int connect_rc = -1;
        std::atomic<bool> connect_returned{false};
        std::thread connection_request([&] {
            connect_rc = connect_server(agent);
            connect_returned.store(true);
        });
        const int local_send_rc =
            send_printer(agent, "studio-serial-1", "M140 S60", 0, 0);
        std::filesystem::create_directory(
            std::filesystem::path(config_dir) / "freshness-local-send-returned"
        );
        const bool local_operation_received = std::filesystem::exists(
            std::filesystem::path(config_dir) / "freshness-local-operation-received"
        );
        version_request.join();
        std::filesystem::create_directory(
            std::filesystem::path(config_dir) / "freshness-version-returned"
        );
        const bool connection_escaped_admission = connect_returned.load();
        std::filesystem::create_directory(
            std::filesystem::path(config_dir) / "freshness-connect-checked"
        );
        refresh_request.join();
        connection_request.join();
        if (local_send_rc != kBambuConnectFailed || local_operation_received ||
            status_count(local, "studio-serial-1") != 0) {
            fail(agent, destroy_agent, "local send escaped while printer freshness was unproven");
        }
        if (connection_escaped_admission || connect_rc != 0) {
            fail(agent, destroy_agent, "connect_server escaped printer cache admission ownership");
        }
        if (version_rc != kBambuConnectFailed || refresh_rc != 0 ||
            version_count(cloud, "studio-serial-1") != 0) {
            fail(agent, destroy_agent, "version callback escaped while printer freshness was unproven");
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(1'100));
        const std::string pushall =
            R"({"pushing":{"command":"pushall","sequence_id":"freshness-retry","version":1,"push_target":1}})";
        if (send_cloud(agent, "studio-serial-1", pushall, 0, 0) != 0 ||
            connected_callbacks.load() != 2) {
            fail(agent, destroy_agent, "unclaimed version response suppressed the next connection handshake");
        }
        destroy_agent(agent);
        print_json(out);
        return 0;
    }
