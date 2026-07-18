#pragma once

#include "shim_state.hpp"

namespace pandar::network_plugin {

std::string printer_telemetry_for(const Agent* agent, const std::string& dev_id) {
    if (!agent) return printer_telemetry_from_json({});
    std::lock_guard<std::mutex> lock(agent->status_mutex);
    if (const auto it = agent->printer_telemetry.find(studio_dev_id(dev_id)); it != agent->printer_telemetry.end()) {
        return it->second;
    }
    return printer_telemetry_from_json({});
}

std::uint32_t studio_ip_integer(const std::string& host) {
    unsigned a = 0;
    unsigned b = 0;
    unsigned c = 0;
    unsigned d = 0;
    char dot1 = 0;
    char dot2 = 0;
    char dot3 = 0;
    std::istringstream stream(host);
    if (!(stream >> a >> dot1 >> b >> dot2 >> c >> dot3 >> d)) return 0;
    if (dot1 != '.' || dot2 != '.' || dot3 != '.') return 0;
    if (a > 255 || b > 255 || c > 255 || d > 255) return 0;
    return static_cast<std::uint32_t>(a | (b << 8) | (c << 16) | (d << 24));
}

std::string camera_url_for(const Agent* agent, const std::string& dev_id) {
    if (!agent || dev_id.empty() || agent->hub_url.empty()) return {};
    const auto [host, access_code] = printer_connection_for(agent, dev_id);
    if (!host.empty() && !access_code.empty()) {
        return "bambu:///rtsps___bblp:" + access_code + "@" + host + "/streaming/live/1?proto=rtsps";
    }
    const auto tenant_id = field_from_json(agent->profile_json, "tenant_id");
    if (tenant_id.empty()) return {};
    return agent->hub_url + "/api/v1/tenants/" + tenant_id + "/printers/" + pandar_printer_id_for(agent, dev_id) + "/camera.mp4";
}

std::string printer_push_status_report(const Agent* agent, const std::string& dev_id) {
    const auto [host, access_code] = printer_connection_for(agent, dev_id);
    const auto rtsp_url = "rtsps://bblp:" + access_code + "@" + host + "/streaming/live/1";
    const auto ip = studio_ip_integer(host);
    return std::string(R"({"print":{"command":"push_status","msg":0,)")
        + printer_telemetry_for(agent, dev_id) +
        R"(,"wifi_signal":"100%","sdcard":true,"ipcam":{"ipcam_dev":"1","liveview":{"local":"rtsps","remote":"none"},"rtsp_url":)" +
        escape_json(rtsp_url) + R"(},"net":{"info":[{"ip":)" + std::to_string(ip) + R"(}]}}})";
}

std::string printer_alive_report(const Agent* agent, const std::string& dev_id) {
    const auto [host, _] = printer_connection_for(agent, dev_id);
    const auto model = printer_model_for(agent, dev_id);
    return std::string(R"({"dev_name":)") + escape_json(dev_id) +
        R"(,"dev_id":)" + escape_json(studio_dev_id(dev_id)) +
        R"(,"dev_ip":)" + escape_json(host) +
        R"(,"dev_type":)" + escape_json(model) +
        R"(,"dev_signal":"100%","connect_type":"lan","bind_state":"free","sec_link":"secure","ssdp_version":"1"})";
}

void emit_cloud_printer_connected_signal(Agent* agent, const std::string& dev_id) {
    if (!agent || dev_id.empty()) return;
    const auto normalized_dev_id = studio_dev_id(dev_id);
    const auto now = std::chrono::steady_clock::now();
    BBL::OnPrinterConnectedFn on_printer_connected;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        if (!agent->on_printer_connected ||
            agent->cloud_initialized_devices.find(normalized_dev_id) !=
                agent->cloud_initialized_devices.end()) {
            return;
        }
        if (const auto notification = agent->cloud_connection_notifications.find(normalized_dev_id);
            notification != agent->cloud_connection_notifications.end() &&
            now - notification->second < std::chrono::seconds(1)) {
            return;
        }
        agent->cloud_connection_notifications[normalized_dev_id] = now;
        on_printer_connected = agent->on_printer_connected;
    }
    const auto topic = std::string("tunnel/") + normalized_dev_id;
    trace_plugin_event(agent, "printer_connected callback=1", topic);
    on_printer_connected(topic);
}

void emit_printer_status(Agent* agent, const std::string& dev_id, MessageTunnel tunnel) {
    if (!agent || dev_id.empty()) return;
    std::uint64_t firmware_generation;
    {
        std::lock_guard<std::recursive_mutex> transition(agent->firmware_transition_mutex);
        firmware_generation = agent->firmware_generation;
    }
    const auto report = printer_push_status_report(agent, dev_id);
    trace_plugin_event(agent, "push_status", dev_id);
    auto callback = message_callback_for(agent, tunnel);
    trace_plugin_event(
        agent,
        std::string("push_status callbacks dev_id=") + dev_id +
            " tunnel=" + (tunnel == MessageTunnel::Cloud ? "cloud" : "local") +
            " callback=" + (callback ? "1" : "0"));
    invoke_message_callback(agent, callback, dev_id, report);
    if (!callback) return;
    const auto normalized_dev_id = studio_dev_id(dev_id);
    std::lock_guard<std::timed_mutex> callback_lock(agent->callback_mutex);
    std::lock_guard<std::recursive_mutex> transition(agent->firmware_transition_mutex);
    if (agent->firmware_generation != firmware_generation) return;
    auto firmware = pandar_plugin_firmware_next_status_override(
        agent->firmware_session,
        reinterpret_cast<const uint8_t*>(normalized_dev_id.data()),
        normalized_dev_id.size()
    );
    auto firmware_body = body_from_result(firmware);
    if (firmware.status == 0) {
        callback(dev_id, firmware_body);
    }
}

void emit_printer_version(
    Agent* agent,
    const std::string& dev_id,
    const std::string& sequence_id,
    MessageTunnel tunnel
) {
    if (!agent || dev_id.empty()) return;
    std::uint64_t firmware_generation;
    {
        std::lock_guard<std::recursive_mutex> transition(agent->firmware_transition_mutex);
        firmware_generation = agent->firmware_generation;
    }
    auto callback = message_callback_for(agent, tunnel);
    trace_plugin_event(
        agent,
        std::string("get_version_response dev_id=") + dev_id +
            " tunnel=" + (tunnel == MessageTunnel::Cloud ? "cloud" : "local") +
            " callback=" + (callback ? "1" : "0"));
    const auto printer_id = pandar_printer_id_for(agent, dev_id);
    auto version = pandar_plugin_firmware_refresh_version(
        agent->firmware_session,
        reinterpret_cast<const uint8_t*>(dev_id.data()),
        dev_id.size(),
        reinterpret_cast<const uint8_t*>(printer_id.data()),
        printer_id.size(),
        reinterpret_cast<const uint8_t*>(sequence_id.data()),
        sequence_id.size()
    );
    auto version_body = body_from_result(version);
    if (!callback) return;
    std::lock_guard<std::timed_mutex> callback_lock(agent->callback_mutex);
    std::lock_guard<std::recursive_mutex> transition(agent->firmware_transition_mutex);
    if (agent->firmware_generation != firmware_generation) return;
    callback(dev_id, version_body);
}

void emit_cloud_printer_connected_status(Agent* agent, const std::string& dev_id) {
    emit_cloud_printer_connected_signal(agent, dev_id);
    emit_printer_status(agent, dev_id, MessageTunnel::Cloud);
}

void emit_cloud_printer_connected_statuses(Agent* agent, const std::vector<std::string>& dev_ids) {
    for (const auto& dev_id : dev_ids) {
        emit_cloud_printer_connected_status(agent, dev_id);
    }
}

void emit_local_connect(Agent* agent, const std::string& dev_id) {
    if (!agent || dev_id.empty()) return;
    BBL::OnLocalConnectedFn on_local_connect;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        on_local_connect = agent->on_local_connect;
    }
    trace_plugin_event(
        agent,
        std::string("local_connect dev_id=") + dev_id +
            " callback=" + (on_local_connect ? "1" : "0"));
    if (on_local_connect) on_local_connect(0, dev_id, printer_alive_report(agent, dev_id));
}

bool handle_status_request(
    Agent* agent,
    const std::string& dev_id,
    const std::string& message,
    MessageTunnel tunnel
) {
    auto request = pandar_plugin_classify_status_request(
        reinterpret_cast<const uint8_t*>(message.data()),
        message.size()
    );
    const auto request_kind = request.status;
    const auto sequence_id = body_from_result(request);
    if (request_kind == kStatusRequestGetVersion) {
        if (tunnel == MessageTunnel::Cloud) {
            std::lock_guard<std::mutex> lock(agent->status_mutex);
            const auto normalized_dev_id = studio_dev_id(dev_id);
            agent->cloud_initialized_devices.insert(normalized_dev_id);
            agent->cloud_connection_notifications.erase(normalized_dev_id);
        }
        emit_printer_version(
            agent,
            dev_id,
            sequence_id,
            tunnel
        );
        return true;
    }
    if (request_kind == kStatusRequestPushAll) {
        refresh_printer_status_cache(agent);
        if (tunnel == MessageTunnel::Cloud) {
            emit_cloud_printer_connected_signal(agent, dev_id);
        }
        emit_printer_status(agent, dev_id, tunnel);
        return true;
    }
    return false;
}

struct StatusHeartbeatTargets {
    std::vector<std::string> cloud;
    std::string local;
};

StatusHeartbeatTargets status_heartbeat_targets(Agent* agent) {
    StatusHeartbeatTargets targets;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        targets.cloud.assign(
            agent->cloud_subscribed_devices.begin(),
            agent->cloud_subscribed_devices.end()
        );
        targets.local = agent->active_local_device;
    }
    return targets;
}

bool has_status_heartbeat_listener(Agent* agent) {
    std::lock_guard<std::mutex> lock(agent->status_mutex);
    return (!agent->cloud_subscribed_devices.empty() && static_cast<bool>(agent->on_message)) ||
        (!agent->active_local_device.empty() && static_cast<bool>(agent->on_local_message));
}

void start_status_heartbeat(Agent* agent) {
    if (!agent || agent->status_thread.joinable()) return;
    agent->status_thread_stop = false;
    agent->status_thread = std::thread([agent] {
        while (!agent->status_thread_stop.load()) {
            std::unique_lock<std::mutex> wait_lock(agent->status_thread_mutex);
            if (agent->status_thread_wake.wait_for(
                    wait_lock,
                    std::chrono::seconds(2),
                    [agent] { return agent->status_thread_stop.load(); }
                )) break;
            wait_lock.unlock();
            if (has_status_heartbeat_listener(agent)) {
                refresh_printer_status_cache(agent);
            }
            auto targets = status_heartbeat_targets(agent);
            emit_cloud_printer_connected_statuses(agent, targets.cloud);
            if (!targets.local.empty()) {
                emit_printer_status(agent, targets.local, MessageTunnel::Local);
            }
        }
    });
}

void stop_status_heartbeat(Agent* agent) {
    if (!agent) return;
    agent->status_thread_stop = true;
    agent->status_thread_wake.notify_all();
    if (agent->status_thread.joinable()) agent->status_thread.join();
}

std::string object_from_json(const std::string& json, const char* key) {
    const std::string needle = std::string("\"") + key + "\"";
    const auto key_pos = json.find(needle);
    if (key_pos == std::string::npos) return {};
    const auto colon = json.find(':', key_pos + needle.size());
    if (colon == std::string::npos) return {};
    const auto start = json.find('{', colon + 1);
    if (start == std::string::npos) return {};
    int depth = 0;
    bool in_string = false;
    bool escaped = false;
    for (std::size_t i = start; i < json.size(); ++i) {
        const char c = json[i];
        if (escaped) {
            escaped = false;
            continue;
        }
        if (c == '\\' && in_string) {
            escaped = true;
            continue;
        }
        if (c == '"') {
            in_string = !in_string;
            continue;
        }
        if (in_string) continue;
        if (c == '{') ++depth;
        if (c == '}') {
            --depth;
            if (depth == 0) return json.substr(start, i - start + 1);
        }
    }
    return {};
}

void apply_profile_json(Agent* agent, const std::string& json) {
    agent->profile_json = json;
    if (const auto v = field_from_json(json, "token"); !v.empty()) agent->token = v;
    if (const auto v = field_from_json(json, "uidStr"); !v.empty()) agent->user_id = v;
    if (const auto v = field_from_json(json, "name"); !v.empty()) agent->user_name = v;
    if (const auto v = field_from_json(json, "user_id"); !v.empty()) agent->user_id = v;
    if (const auto v = field_from_json(json, "user_name"); !v.empty()) agent->user_name = v;
    if (const auto v = field_from_json(json, "tenant_name"); !v.empty() && agent->user_name.empty()) agent->user_name = v;
    sync_printer_refresh_session(agent);
}

void persist_login_state(Agent* agent) {
    const auto path = persisted_login_path(agent);
    if (path.empty() || !agent || agent->token.empty() || agent->profile_json.empty()) return;
    std::error_code error;
    std::filesystem::create_directories(path.parent_path(), error);
    if (error) return;

    auto temp_path = path;
    temp_path += ".tmp";
    std::ofstream file(temp_path, std::ios::binary | std::ios::trunc);
    if (!file) return;
    file << "{"
         << "\"hub_url\":" << escape_json(agent->hub_url) << ","
         << "\"token\":" << escape_json(agent->token) << ","
         << "\"profile\":" << agent->profile_json
         << "}";
    file.close();
    if (!file) {
        std::filesystem::remove(temp_path, error);
        return;
    }
    std::filesystem::rename(temp_path, path, error);
    if (error) {
        std::filesystem::remove(path, error);
        std::filesystem::rename(temp_path, path, error);
    }
}

void load_persisted_login(Agent* agent) {
    const auto path = persisted_login_path(agent);
    if (path.empty() || !agent || !agent->token.empty()) return;
    std::ifstream file(path, std::ios::binary);
    if (!file) return;
    std::string body((std::istreambuf_iterator<char>(file)), std::istreambuf_iterator<char>());
    if (field_from_json(body, "hub_url") != agent->hub_url) return;
    const auto token = field_from_json(body, "token");
    const auto profile = object_from_json(body, "profile");
    if (token.empty() || profile.empty()) return;
    agent->token = token;
    apply_profile_json(agent, profile);
    if (agent->token.empty()) agent->token = token;
}

std::string studio_token_body(const Agent* agent) {
    const auto token = agent ? agent->token : std::string{};
    return std::string("{") +
           "\"accessToken\":" + escape_json(token) + "," +
           "\"refreshToken\":\"\"," +
           "\"expiresIn\":31536000," +
           "\"refreshExpiresIn\":31536000," +
           "\"tfaKey\":\"\"," +
           "\"accessMethod\":\"pandar\"," +
           "\"loginType\":\"pandar\"" +
           "}";
}

std::string studio_profile_body(const Agent* agent) {
    if (!agent) return R"({"uidStr":"","account":"","name":"","avatar":""})";
    return std::string("{") +
           "\"uidStr\":" + escape_json(agent->user_id) + "," +
           "\"account\":" + escape_json(agent->user_name) + "," +
           "\"name\":" + escape_json(agent->user_name) + "," +
           "\"avatar\":" + escape_json(agent->avatar) +
           "}";
}

std::string body_from_result(PluginHttpResult result) {
    std::string body;
    if (result.body_ptr && result.body_len > 0) {
        body.assign(reinterpret_cast<char*>(result.body_ptr), result.body_len);
        pandar_plugin_free_with_capacity(result.body_ptr, result.body_len, result.body_cap);
    }
    return body;
}


} // namespace pandar::network_plugin
