bool exact_camera_relay_url(const std::string& url) {
    const std::string prefix = "bambu:///local/127.0.0.1?port=";
    if (url.rfind(prefix, 0) != 0) return false;
    const auto auth_separator = url.find("&auth=", prefix.size());
    if (auth_separator == std::string::npos) return false;
    const auto port = url.substr(prefix.size(), auth_separator - prefix.size());
    if (port.empty() || port.size() > 5) return false;
    unsigned int port_number = 0;
    for (const char value : port) {
        if (value < '0' || value > '9') return false;
        port_number = port_number * 10 + static_cast<unsigned int>(value - '0');
    }
    if (port_number == 0 || port_number > 65535) return false;
    const auto auth = url.substr(auth_separator + 6);
    if (auth.size() != 32) return false;
    for (const char value : auth) {
        if (!((value >= '0' && value <= '9') || (value >= 'a' && value <= 'f'))) return false;
    }
    return true;
}

bool all_unique(const std::vector<std::string>& values) {
    for (std::size_t left = 0; left < values.size(); ++left) {
        for (std::size_t right = left + 1; right < values.size(); ++right) {
            if (values[left] == values[right]) return false;
        }
    }
    return true;
}

struct ProbeResult {
    bool ok = true;
    std::string host;
    std::string login_command;
    std::string login_info;
    std::string logout_command;
    int printer_rc = 0;
    int tasks_rc = 0;
    int print_rc = 0;
    int direct_connect_rc = 0;
    int direct_message_rc = 0;
    bool direct_connect_callback = false;
    bool local_connect_callback = false;
    bool message_callback = false;
    bool version_callback = false;
    int selected_machine_messages = 0;
    int subscribe_messages = 0;
    int heartbeat_messages = 0;
    int connect_messages = 0;
    std::string camera_url;
    bool camera_available_exact = false;
    int camera_callback_count = 0;
    int camera_golive_callback_count = 0;
    bool camera_urls_unique = false;
    bool camera_credentials_redacted = false;
    bool camera_unavailable_exact = false;
    bool local_connect_transport_redacted = false;
    int ft_abi_version = 0;
    int ft_start_connect_rc = 0;
    int ft_sync_rc = 0;
    int ft_start_job_rc = 0;
    int ft_job_result_ec = 0;
    int ft_cancel_rc = 0;
    int update_stage = -1;
    int update_code = -1;
    std::string update_body;
    std::string sdcard_update_body;
    bool restored_login = false;
    int cloud_version_messages = 0;
    int cloud_status_messages = 0;
    int local_version_messages = 0;
    int local_status_messages = 0;
    int printer_connected_callbacks = 0;
    int local_connect_callbacks = 0;
    int operation_posts = 0;
    bool status_requests_zero_posts = false;
    bool status_request_lookalikes_exact = false;
    bool native_actions_exact = false;
    bool cloud_unsupported_exact = false;
    bool local_unsupported_exact = false;
    bool empty_cloud_target_rejected_without_refresh_post_or_callbacks = false;
    bool unsupported_commands_rejected_without_post_or_callbacks = false;
    bool unsupported_parser_error_stable = false;
    bool mixed_status_envelopes_rejected_without_callbacks = false;
    std::string command_disposition_failure;
    bool cloud_invalid_native_exact = false;
    bool local_invalid_native_exact = false;
    bool cloud_requests_exact = false;
    bool lan_connect_exact = false;
    bool local_requests_exact = false;
    bool callback_tunnels_exact = false;
    bool local_only_refresh_exact = false;
    bool local_replacement_exact = false;
    bool disconnect_exact = false;
    bool same_serial_exact = false;
    bool axis_features_exact = false;
    std::string cloud_status_body;
    std::string local_status_body;
};

[[noreturn]] void fail(void* agent, int (*destroy_agent)(void*), const std::string& message) {
    if (agent && destroy_agent) destroy_agent(agent);
    std::cerr << message << "\n";
    std::exit(2);
}

void assert_redacted_stable_error(
    void* agent,
    int (*destroy_agent)(void*),
    const std::string& body,
    const std::string& stable_error,
    const std::vector<std::string>& forbidden
) {
    if (!contains(body, stable_error)) {
        fail(agent, destroy_agent, "ABI body did not contain stable error: " + stable_error);
    }
    for (const auto& value : forbidden) {
        if (contains(body, value)) {
            fail(agent, destroy_agent, "ABI body leaked forbidden value: " + value);
        }
    }
}

void print_json(const ProbeResult& result) {
    std::cout
        << "{"
        << "\"ok\":" << (result.ok ? "true" : "false")
        << ",\"host\":" << escape_json(result.host)
        << ",\"login_command\":" << escape_json(result.login_command)
        << ",\"login_info\":" << escape_json(result.login_info)
        << ",\"logout_command\":" << escape_json(result.logout_command)
        << ",\"printer_rc\":" << result.printer_rc
        << ",\"tasks_rc\":" << result.tasks_rc
        << ",\"print_rc\":" << result.print_rc
        << ",\"direct_connect_rc\":" << result.direct_connect_rc
        << ",\"direct_message_rc\":" << result.direct_message_rc
        << ",\"direct_connect_callback\":" << (result.direct_connect_callback ? "true" : "false")
        << ",\"local_connect_callback\":" << (result.local_connect_callback ? "true" : "false")
        << ",\"message_callback\":" << (result.message_callback ? "true" : "false")
        << ",\"version_callback\":" << (result.version_callback ? "true" : "false")
        << ",\"selected_machine_messages\":" << result.selected_machine_messages
        << ",\"subscribe_messages\":" << result.subscribe_messages
        << ",\"heartbeat_messages\":" << result.heartbeat_messages
        << ",\"connect_messages\":" << result.connect_messages
        << ",\"camera_url\":" << escape_json(result.camera_url)
        << ",\"camera_available_exact\":" << (result.camera_available_exact ? "true" : "false")
        << ",\"camera_callback_count\":" << result.camera_callback_count
        << ",\"camera_golive_callback_count\":" << result.camera_golive_callback_count
        << ",\"camera_urls_unique\":" << (result.camera_urls_unique ? "true" : "false")
        << ",\"camera_credentials_redacted\":" << (result.camera_credentials_redacted ? "true" : "false")
        << ",\"camera_unavailable_exact\":" << (result.camera_unavailable_exact ? "true" : "false")
        << ",\"local_connect_transport_redacted\":" << (result.local_connect_transport_redacted ? "true" : "false")
        << ",\"ft_abi_version\":" << result.ft_abi_version
        << ",\"ft_start_connect_rc\":" << result.ft_start_connect_rc
        << ",\"ft_sync_rc\":" << result.ft_sync_rc
        << ",\"ft_start_job_rc\":" << result.ft_start_job_rc
        << ",\"ft_job_result_ec\":" << result.ft_job_result_ec
        << ",\"ft_cancel_rc\":" << result.ft_cancel_rc
        << ",\"update_stage\":" << result.update_stage
        << ",\"update_code\":" << result.update_code
        << ",\"update_body\":" << escape_json(result.update_body)
        << ",\"sdcard_update_body\":" << escape_json(result.sdcard_update_body)
        << ",\"restored_login\":" << (result.restored_login ? "true" : "false")
        << ",\"cloud_version_messages\":" << result.cloud_version_messages
        << ",\"cloud_status_messages\":" << result.cloud_status_messages
        << ",\"local_version_messages\":" << result.local_version_messages
        << ",\"local_status_messages\":" << result.local_status_messages
        << ",\"printer_connected_callbacks\":" << result.printer_connected_callbacks
        << ",\"local_connect_callbacks\":" << result.local_connect_callbacks
        << ",\"operation_posts\":" << result.operation_posts
        << ",\"status_requests_zero_posts\":" << (result.status_requests_zero_posts ? "true" : "false")
        << ",\"status_request_lookalikes_exact\":" << (result.status_request_lookalikes_exact ? "true" : "false")
        << ",\"native_actions_exact\":" << (result.native_actions_exact ? "true" : "false")
        << ",\"cloud_unsupported_exact\":" << (result.cloud_unsupported_exact ? "true" : "false")
        << ",\"local_unsupported_exact\":" << (result.local_unsupported_exact ? "true" : "false")
        << ",\"empty_cloud_target_rejected_without_refresh_post_or_callbacks\":" << (result.empty_cloud_target_rejected_without_refresh_post_or_callbacks ? "true" : "false")
        << ",\"unsupported_commands_rejected_without_post_or_callbacks\":" << (result.unsupported_commands_rejected_without_post_or_callbacks ? "true" : "false")
        << ",\"unsupported_parser_error_stable\":" << (result.unsupported_parser_error_stable ? "true" : "false")
        << ",\"mixed_status_envelopes_rejected_without_callbacks\":" << (result.mixed_status_envelopes_rejected_without_callbacks ? "true" : "false")
        << ",\"command_disposition_failure\":" << escape_json(result.command_disposition_failure)
        << ",\"cloud_invalid_native_exact\":" << (result.cloud_invalid_native_exact ? "true" : "false")
        << ",\"local_invalid_native_exact\":" << (result.local_invalid_native_exact ? "true" : "false")
        << ",\"cloud_requests_exact\":" << (result.cloud_requests_exact ? "true" : "false")
        << ",\"lan_connect_exact\":" << (result.lan_connect_exact ? "true" : "false")
        << ",\"local_requests_exact\":" << (result.local_requests_exact ? "true" : "false")
        << ",\"callback_tunnels_exact\":" << (result.callback_tunnels_exact ? "true" : "false")
        << ",\"local_only_refresh_exact\":" << (result.local_only_refresh_exact ? "true" : "false")
        << ",\"local_replacement_exact\":" << (result.local_replacement_exact ? "true" : "false")
        << ",\"disconnect_exact\":" << (result.disconnect_exact ? "true" : "false")
        << ",\"same_serial_exact\":" << (result.same_serial_exact ? "true" : "false")
        << ",\"axis_features_exact\":" << (result.axis_features_exact ? "true" : "false")
        << ",\"cloud_status_body\":" << escape_json(result.cloud_status_body)
        << ",\"local_status_body\":" << escape_json(result.local_status_body)
        << "}\n";
}

void ft_connect_cb(void* user, int, int, const char* msg) {
    auto* out = reinterpret_cast<std::string*>(user);
    *out = msg ? msg : "";
}

void ft_result_cb(void* user, ft_job_result result) {
    auto* out = reinterpret_cast<int*>(user);
    *out = result.ec;
}

struct TunnelCapture {
    std::atomic<int> version_messages{0};
    std::atomic<int> status_messages{0};
    std::atomic<int> disconnected_messages{0};
    std::mutex mutex;
    std::map<std::string, int> version_by_device;
    std::map<std::string, int> status_by_device;
    std::map<std::string, int> disconnected_by_device;
    std::map<std::string, std::string> status_body_by_device;
};

void capture_tunnel_message(TunnelCapture& capture, const std::string& dev_id, const std::string& body) {
    const bool is_version = contains(body, R"("command":"get_version")");
    const bool is_status = contains(body, R"("command":"push_status")");
    const bool is_disconnected = body == R"({"event":{"event":"client.disconnected"}})";
    if (!is_version && !is_status && !is_disconnected) return;
    std::lock_guard<std::mutex> lock(capture.mutex);
    if (is_version) {
        ++capture.version_messages;
        ++capture.version_by_device[dev_id];
    }
    if (is_status) {
        ++capture.status_messages;
        ++capture.status_by_device[dev_id];
        capture.status_body_by_device[dev_id] = body;
    }
    if (is_disconnected) {
        ++capture.disconnected_messages;
        ++capture.disconnected_by_device[dev_id];
    }
}

int status_count(TunnelCapture& capture, const std::string& dev_id) {
    std::lock_guard<std::mutex> lock(capture.mutex);
    return capture.status_by_device[dev_id];
}

int version_count(TunnelCapture& capture, const std::string& dev_id) {
    std::lock_guard<std::mutex> lock(capture.mutex);
    return capture.version_by_device[dev_id];
}

int disconnected_count(TunnelCapture& capture, const std::string& dev_id) {
    std::lock_guard<std::mutex> lock(capture.mutex);
    return capture.disconnected_by_device[dev_id];
}

std::string status_body(TunnelCapture& capture, const std::string& dev_id) {
    std::lock_guard<std::mutex> lock(capture.mutex);
    return capture.status_body_by_device[dev_id];
}

bool wait_until(const std::function<bool()>& ready) {
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::milliseconds(3500);
    while (!ready() && std::chrono::steady_clock::now() < deadline) {
        std::this_thread::sleep_for(std::chrono::milliseconds(25));
    }
    return ready();
}

int hub_operation_count() {
    const char* hub_url = std::getenv("PANDAR_PLUGIN_HUB_URL");
    if (!hub_url || hub_url[0] == '\0') return -1;
    const auto response = http_request(
        hub_url,
        "GET /probe-operation-count HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    );
    const auto body = http_body(response);
    const auto marker = body.find(R"("count":)");
    if (marker == std::string::npos) return -1;
    return std::stoi(body.substr(marker + 8));
}

int hub_printer_refresh_count() {
    const char* hub_url = std::getenv("PANDAR_PLUGIN_HUB_URL");
    if (!hub_url || hub_url[0] == '\0') return -1;
    const auto response = http_request(
        hub_url,
        "GET /probe-printer-refresh-count HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    );
    const auto body = http_body(response);
    const auto marker = body.find(R"("count":)");
    if (marker == std::string::npos) return -1;
    return std::stoi(body.substr(marker + 8));
}

int hub_presence_step() {
    const char* hub_url = std::getenv("PANDAR_PLUGIN_HUB_URL");
    if (!hub_url || hub_url[0] == '\0') return -1;
    const auto response = http_request(
        hub_url,
        "GET /probe-presence-step HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    );
    const auto body = http_body(response);
    const auto marker = body.find(R"("step":)");
    if (marker == std::string::npos) return -1;
    return std::stoi(body.substr(marker + 7));
}

} // namespace
