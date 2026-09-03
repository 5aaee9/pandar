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
    const bool stream_unavailable_mode = mode == "stream-unavailable";
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
    const bool server_selection_restore_mode = mode == "server-selection-restore";
    const bool server_selection_child_restore_mode = mode == "server-selection-restore-child";
    const bool server_selection_override_mode = mode == "server-selection-restore-override";
    const bool server_selection_mode =
        server_selection_restore_mode || server_selection_child_restore_mode ||
        server_selection_override_mode;
    if (!failure_mode && !stale_token_mode && !native_print_error_mode && !axis_features_mode &&
        !connection_readiness_mode && !background_timeout_mode && !stream_unavailable_mode &&
        !auth_rejected_mode &&
        !printer_presence_mode && !account_transition_mode && !account_exchange_race_mode &&
        !token_refresh_mode &&
        !freshness_claim_mode && !firmware_claim_race_mode && !callback_order_mode &&
        !request_admission_mode && !camera_available_mode &&
        !camera_unavailable_mode && !no_auth_recovery_mode &&
        !official_no_auth_lifecycle_mode && !server_selection_mode &&
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
