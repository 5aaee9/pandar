#pragma once

#include "shim_types.hpp"

namespace pandar::network_plugin {

Agent* as_agent(void* raw) {
    return reinterpret_cast<Agent*>(raw);
}

void trace_plugin_event(const Agent* agent, const std::string& message) {
    if (!agent) return;
    std::filesystem::path base;
    {
        std::lock_guard<std::mutex> lock(agent->trace_mutex);
        base = !agent->config_dir.empty() ? std::filesystem::path(agent->config_dir)
                                          : std::filesystem::path(agent->log_dir);
    }
    if (base.empty()) return;
    std::error_code ec;
    std::filesystem::create_directories(base, ec);
    std::ofstream out(base / "pandar-network-plugin.trace.log", std::ios::app);
    if (out) out << message << '\n';
}

void trace_plugin_event(const Agent* agent, const std::string& event, const std::string& dev_id) {
    trace_plugin_event(agent, event + " dev_id=" + dev_id);
}

std::string body_from_result(PluginHttpResult result);
void refresh_local_webserver_config(Agent* agent);
PluginHttpResult rust_get_printers(const Agent* agent);
PluginHttpResult get_printers_with_token_refresh(
    Agent*, std::uint64_t& request_epoch, FirmwareObservationTicket& observation
);
void sync_printer_refresh_session(Agent* agent);
FirmwareObservationTicket begin_firmware_observation(Agent* agent);
bool observe_firmware_printers(
    Agent*, const std::string& body, const FirmwareObservationTicket& observation
);
BBL::OnMessageFn message_callback_for(Agent* agent, MessageTunnel tunnel);
void invoke_message_callback(
    Agent*, const BBL::OnMessageFn&, const std::string&, const std::string&
);

struct FirmwareObservationReservation {
    Agent* agent;
    FirmwareObservationTicket* observation;
};

extern "C" void reserve_firmware_observation(void* context) noexcept {
    auto* reservation = static_cast<FirmwareObservationReservation*>(context);
    *reservation->observation = begin_firmware_observation(reservation->agent);
}

bool has_hub(const Agent* agent) {
    return agent && !agent->hub_url.empty();
}

void clear_login_state(Agent* agent, bool sync_sessions = true) {
    agent->printer_status_epoch.fetch_add(1);
    agent->token.clear();
    agent->user_id.clear();
    agent->user_name.clear();
    agent->avatar.clear();
    agent->profile_json.clear();
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        agent->selected_machine.clear();
        agent->printer_connections.clear();
        agent->pandar_printer_ids.clear();
        agent->printer_models.clear();
        agent->printer_telemetry.clear();
        agent->cloud_subscribed_devices.clear();
        agent->cloud_initialized_devices.clear();
        agent->cloud_connection_notifications.clear();
    }
    agent->connected = false;
    if (sync_sessions) sync_printer_refresh_session(agent);
}

std::filesystem::path persisted_login_path(const Agent* agent) {
    if (!agent || agent->config_dir.empty()) return {};
    return std::filesystem::path(agent->config_dir) / "pandar-plugin-login.json";
}

void clear_persisted_login(Agent* agent) {
    const auto path = persisted_login_path(agent);
    if (path.empty()) return;
    std::error_code ignored;
    std::filesystem::remove(path, ignored);
}

std::pair<std::string, bool> env_or_default(const char* primary, const char* secondary, std::string fallback) {
    if (const char* value = std::getenv(primary); value && value[0] != '\0') {
        return {value, true};
    }
    if (const char* value = std::getenv(secondary); value && value[0] != '\0') {
        return {value, true};
    }
    return {std::move(fallback), false};
}

std::string escape_json(const std::string& value) {
    std::string out;
    out.reserve(value.size() + 2);
    out.push_back('"');
    for (char c : value) {
        switch (c) {
            case '\\': out += "\\\\"; break;
            case '"': out += "\\\""; break;
            case '\n': out += "\\n"; break;
            case '\r': out += "\\r"; break;
            case '\t': out += "\\t"; break;
            default: out.push_back(c); break;
        }
    }
    out.push_back('"');
    return out;
}

std::string field_from_json(const std::string& json, const char* key) {
    const std::string needle = std::string("\"") + key + "\"";
    const auto key_pos = json.find(needle);
    if (key_pos == std::string::npos) return {};
    const auto colon = json.find(':', key_pos + needle.size());
    if (colon == std::string::npos) return {};
    const auto quote = json.find_first_not_of(" \t\r\n", colon + 1);
    if (quote == std::string::npos || json[quote] != '"') return {};
    std::string out;
    for (std::size_t i = quote + 1; i < json.size(); ++i) {
        const char c = json[i];
        if (c == '\\' && i + 1 < json.size()) {
            out.push_back(json[++i]);
            continue;
        }
        if (c == '"') break;
        out.push_back(c);
    }
    return out;
}

std::vector<std::string> objects_from_array(const std::string& json, const char* key);
std::string object_from_json(const std::string& json, const char* key);

std::string printer_telemetry_from_json(const std::string& printer) {
    auto result = pandar_plugin_printer_telemetry_json(
        reinterpret_cast<const uint8_t*>(printer.data()),
        printer.size()
    );
    return body_from_result(result);
}

std::string studio_dev_id(std::string dev_id) {
    if (const auto separator = dev_id.find('|'); separator != std::string::npos) {
        dev_id.resize(separator);
    }
    return dev_id;
}

std::vector<std::string> objects_from_array(const std::string& json, const char* key) {
    const std::string needle = std::string("\"") + key + "\"";
    const auto key_pos = json.find(needle);
    if (key_pos == std::string::npos) return {};
    const auto colon = json.find(':', key_pos + needle.size());
    if (colon == std::string::npos) return {};
    const auto start = json.find('[', colon + 1);
    if (start == std::string::npos) return {};
    int array_depth = 0;
    int object_depth = 0;
    bool in_string = false;
    bool escaped = false;
    std::size_t object_start = std::string::npos;
    std::vector<std::string> objects;
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
        if (c == '[') {
            ++array_depth;
            continue;
        }
        if (c == ']') {
            --array_depth;
            if (array_depth == 0) break;
            continue;
        }
        if (array_depth != 1) continue;
        if (c == '{') {
            if (object_depth == 0) object_start = i;
            ++object_depth;
            continue;
        }
        if (c == '}') {
            --object_depth;
            if (object_depth == 0 && object_start != std::string::npos) {
                objects.push_back(json.substr(object_start, i - object_start + 1));
                object_start = std::string::npos;
            }
        }
    }
    return objects;
}

bool remember_printer_connections(
    Agent* agent,
    const std::string& body,
    std::uint64_t expected_epoch
) {
    if (!agent) return false;
    std::map<std::string, std::pair<std::string, std::string>> printer_connections;
    std::map<std::string, std::string> pandar_printer_ids;
    std::map<std::string, std::string> printer_models;
    std::map<std::string, std::string> printer_telemetry;
    for (const auto& printer : objects_from_array(body, "devices")) {
        const auto dev_id = field_from_json(printer, "dev_id");
        if (dev_id.empty()) continue;
        printer_connections[dev_id] = {
            field_from_json(printer, "dev_ip"),
            field_from_json(printer, "dev_access_code"),
        };
        if (const auto pandar_id = field_from_json(printer, "pandar_printer_id"); !pandar_id.empty()) {
            pandar_printer_ids[dev_id] = pandar_id;
        }
        if (const auto model = field_from_json(printer, "dev_model_name"); !model.empty()) {
            printer_models[dev_id] = model;
        }
        printer_telemetry[dev_id] = printer_telemetry_from_json(printer);
    }
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        if (agent->printer_status_epoch.load() != expected_epoch) {
            return false;
        }
        agent->printer_connections.swap(printer_connections);
        agent->pandar_printer_ids.swap(pandar_printer_ids);
        agent->printer_models.swap(printer_models);
        agent->printer_telemetry.swap(printer_telemetry);
    }
    return true;
}

bool refresh_printer_status_cache(Agent* agent) {
    if (!agent || !agent->printer_refresh_session) return false;
    const auto epoch = agent->printer_status_epoch.load();
    FirmwareObservationTicket observation;
    FirmwareObservationReservation reservation{agent, &observation};
    auto result = pandar_plugin_printer_refresh(
        agent->printer_refresh_session,
        &reservation,
        reserve_firmware_observation
    );
    const auto status = result.status;
    const auto http_code = result.http_code;
    auto body = body_from_result(result);
    if (status != 0) {
        trace_plugin_event(
            agent,
            "printer status refresh failed status=" + std::to_string(status) +
                " http_code=" + std::to_string(http_code) + " body=" + body
        );
        return false;
    }
    const auto remembered = remember_printer_connections(agent, body, epoch);
    if (remembered) observe_firmware_printers(agent, body, observation);
    return remembered;
}

std::pair<std::string, std::string> printer_connection_for(const Agent* agent, const std::string& dev_id) {
    if (!agent) return {};
    std::lock_guard<std::mutex> lock(agent->status_mutex);
    if (const auto it = agent->printer_connections.find(studio_dev_id(dev_id)); it != agent->printer_connections.end()) {
        return it->second;
    }
    return {};
}

std::string pandar_printer_id_for(const Agent* agent, const std::string& dev_id) {
    if (!agent) return studio_dev_id(dev_id);
    std::lock_guard<std::mutex> lock(agent->status_mutex);
    const auto normalized = studio_dev_id(dev_id);
    if (const auto it = agent->pandar_printer_ids.find(normalized); it != agent->pandar_printer_ids.end()) {
        return it->second;
    }
    return normalized;
}

std::string printer_model_for(const Agent* agent, const std::string& dev_id) {
    if (!agent) return "C11";
    std::lock_guard<std::mutex> lock(agent->status_mutex);
    if (const auto it = agent->printer_models.find(studio_dev_id(dev_id)); it != agent->printer_models.end()) {
        return it->second;
    }
    return "C11";
}

std::string first_known_printer_id(Agent* agent) {
    if (!agent) return {};
    std::lock_guard<std::mutex> lock(agent->status_mutex);
    if (!agent->printer_connections.empty()) return agent->printer_connections.begin()->first;
    if (!agent->cloud_subscribed_devices.empty()) return *agent->cloud_subscribed_devices.begin();
    return {};
}

std::string ensure_selected_machine(Agent* agent) {
    if (!agent) return {};
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        if (!agent->selected_machine.empty()) return agent->selected_machine;
    }

    auto selected = first_known_printer_id(agent);
    if (selected.empty() && !agent->token.empty()) {
        refresh_local_webserver_config(agent);
        std::uint64_t request_epoch = 0;
        FirmwareObservationTicket observation;
        auto result = get_printers_with_token_refresh(agent, request_epoch, observation);
        auto body = body_from_result(result);
        if (result.status == 0 && remember_printer_connections(agent, body, request_epoch)) {
            observe_firmware_printers(agent, body, observation);
            selected = first_known_printer_id(agent);
        }
    }

    if (!selected.empty()) {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        if (agent->selected_machine.empty()) agent->selected_machine = selected;
        agent->cloud_subscribed_devices.insert(selected);
        return agent->selected_machine;
    }
    return {};
}


} // namespace pandar::network_plugin
