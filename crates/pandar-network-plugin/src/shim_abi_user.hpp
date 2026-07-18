#pragma once

#include "shim_firmware.hpp"

namespace pandar::network_plugin {

std::string login_envelope(const Agent* agent, bool logout) {
    if (logout || !agent || agent->token.empty()) {
        return R"({"sequence_id":"0","command":"studio_useroffline","data":{}})";
    }
    return std::string(R"({"sequence_id":"0","command":"studio_userlogin","data":{)") +
           "\"avatar\":" + escape_json(agent->avatar) + "," +
           "\"name\":" + escape_json(agent->user_name) + "," +
           "\"user_id\":" + escape_json(agent->user_id) + "," +
           "\"user_name\":" + escape_json(agent->user_name) + "," +
           "\"nickname\":" + escape_json(agent->user_name) + "," +
           "\"account\":" + escape_json(agent->user_name) + "," +
           "\"token\":" + escape_json(agent->token) + "," +
           R"("refresh":""}})";
}

std::string profile_body(const Agent* agent) {
    if (!agent || agent->profile_json.empty()) return R"({"user_id":"","user_name":"","tenant_id":"","tenant_name":""})";
    return agent->profile_json;
}

void success_body(unsigned int* http_code, std::string* http_body, std::string body) {
    if (http_code) *http_code = 200;
    if (http_body) *http_body = std::move(body);
}


} // namespace pandar::network_plugin

using namespace pandar::network_plugin;


PANDAR_IGNORE_CXX_LINKAGE_BEGIN

PANDAR_ABI std::string bambu_network_get_version() {
    return "02.07.01.00";
}

PANDAR_ABI std::string bambu_network_get_user_id(void* agent) {
    auto* a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    return a ? a->user_id : std::string{};
}

PANDAR_ABI std::string bambu_network_get_user_name(void* agent) {
    auto* a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    return a ? a->user_name : std::string{};
}

PANDAR_ABI std::string bambu_network_get_user_avatar(void* agent) {
    auto* a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    return a ? a->avatar : std::string{};
}

PANDAR_ABI std::string bambu_network_get_user_nickanme(void* agent) {
    auto* a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    return a ? a->user_name : std::string{};
}

PANDAR_ABI std::string bambu_network_build_login_cmd(void* agent) {
    auto* a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    return login_envelope(a, false);
}

PANDAR_ABI std::string bambu_network_build_logout_cmd(void* agent) {
    return login_envelope(as_agent(agent), true);
}

PANDAR_ABI std::string bambu_network_build_login_info(void* agent) {
    auto* a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    return login_envelope(a, false);
}

PANDAR_ABI std::string bambu_network_get_bambulab_host(void* agent) {
    auto* a = as_agent(agent);
    if (!a) return {};
    auto result = rust_start_local_webserver(a);
    std::string body = body_from_result(result);
    if (result.status != 0) {
        a->last_error = body;
        return {};
    }
    if (const auto base_url = field_from_json(body, "base_url"); !base_url.empty()) {
        a->last_error.clear();
        return base_url;
    }
    a->last_error = R"({"error":"local_webserver_unavailable"})";
    return {};
}

PANDAR_ABI std::string bambu_network_get_user_selected_machine(void* agent) {
    auto* a = as_agent(agent);
    if (!a) return {};
    auto selected = ensure_selected_machine(a);
    trace_plugin_event(a, std::string("get_user_selected_machine selected=") + selected);
    return selected;
}

PANDAR_ABI std::string bambu_network_get_studio_info_url(void*) {
    return {};
}

PANDAR_ABI std::string bambu_network_request_setting_id(void*, std::string, std::map<std::string, std::string>*, unsigned int* http_code) {
    if (http_code) *http_code = 0;
    return {};
}

PANDAR_IGNORE_CXX_LINKAGE_END

PANDAR_ABI bool bambu_network_check_debug_consistent(bool) {
    return true;
}

PANDAR_ABI void* bambu_network_create_agent(std::string log_dir) {
    auto* agent = new Agent(std::move(log_dir));
    auto [hub_url, hub_configured] = env_or_default("PANDAR_PLUGIN_HUB_URL", "APP_API_URL", "http://127.0.0.1:8080");
    auto [frontend_url, frontend_configured] = env_or_default("PANDAR_PLUGIN_FRONTEND_URL", "APP_BASE_URL", "http://localhost:3000");
    agent->hub_url = std::move(hub_url);
    agent->frontend_url = std::move(frontend_url);
    agent->hub_configured = hub_configured;
    agent->frontend_configured = frontend_configured;
    agent->printer_refresh_session = pandar_plugin_printer_refresh_session_create(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size()
    );
    agent->firmware_session = pandar_plugin_firmware_session_create(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size(),
        agent->firmware_generation
    );
    agent->firmware_hub_url = agent->hub_url;
    agent->firmware_token = agent->token;
    if (agent->firmware_session) start_firmware_dispatcher(agent);
    start_status_heartbeat(agent);
    return agent;
}

PANDAR_ABI int bambu_network_destroy_agent(void* agent) {
    auto* a = as_agent(agent);
    stop_status_heartbeat(a);
    stop_firmware_dispatcher(a);
    if (a) {
        pandar_plugin_firmware_session_destroy(a->firmware_session);
        pandar_plugin_printer_refresh_session_destroy(a->printer_refresh_session);
        std::lock_guard<std::mutex> lock(a->status_mutex);
        a->on_message = {};
        a->on_local_message = {};
        a->on_printer_connected = {};
        a->on_server_connected = {};
        a->on_local_connect = {};
    }
    delete a;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_init_log(void*) {
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_config_dir(void* agent, std::string config_dir) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    {
        std::lock_guard<std::mutex> lock(a->trace_mutex);
        a->config_dir = std::move(config_dir);
    }
    load_persisted_login(a);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_cert_file(void* agent, std::string folder, std::string filename) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    a->cert_folder = std::move(folder);
    a->cert_filename = std::move(filename);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_country_code(void* agent, std::string country_code) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    a->country_code = std::move(country_code);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_start(void* agent) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    auto result = rust_start_local_webserver(a);
    if (result.status != 0) {
        a->last_error = body_from_result(result);
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    a->last_error.clear();
    try_no_auth_session(a);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

#define PANDAR_CALLBACK_SETTER(name, type) \
    PANDAR_ABI int name(void* agent, BBL::type) { \
        return as_agent(agent) ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE; \
    }

PANDAR_CALLBACK_SETTER(bambu_network_set_on_ssdp_msg_fn, OnMsgArrivedFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_user_login_fn, OnUserLoginFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_http_error_fn, OnHttpErrorFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_get_country_code_fn, GetCountryCodeFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_subscribe_failure_fn, GetSubscribeFailureFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_user_message_fn, OnMessageFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_queue_on_main_fn, QueueOnMainFn)
PANDAR_CALLBACK_SETTER(bambu_network_set_server_callback, OnServerErrFn)

#undef PANDAR_CALLBACK_SETTER

PANDAR_ABI int bambu_network_set_on_server_connected_fn(void* agent, BBL::OnServerConnectedFn callback) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    a->on_server_connected = std::move(callback);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_on_message_fn(void* agent, BBL::OnMessageFn callback) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    a->on_message = std::move(callback);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_on_local_message_fn(void* agent, BBL::OnMessageFn callback) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    a->on_local_message = std::move(callback);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_on_printer_connected_fn(void* agent, BBL::OnPrinterConnectedFn callback) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    a->on_printer_connected = std::move(callback);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_on_local_connect_fn(void* agent, BBL::OnLocalConnectedFn callback) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    a->on_local_connect = std::move(callback);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_connect_server(void* agent) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (a->connected && has_hub(a) && a->last_error.empty()) return BBL::BAMBU_NETWORK_SUCCESS;
    {
        std::lock_guard<std::mutex> lock(a->status_mutex);
        a->cloud_initialized_devices.clear();
        a->cloud_connection_notifications.clear();
    }
    a->connected = has_hub(a);
    if (a->connected) a->last_error.clear();
    BBL::OnServerConnectedFn on_server_connected;
    {
        std::lock_guard<std::mutex> lock(a->status_mutex);
        on_server_connected = a->on_server_connected;
    }
    if (on_server_connected) {
        on_server_connected(a->connected ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED, 0);
    }
    return a->connected ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED;
}

PANDAR_ABI bool bambu_network_is_server_connected(void* agent) {
    auto* a = as_agent(agent);
    return a && a->connected && has_hub(a) && a->last_error.empty();
}

PANDAR_ABI int bambu_network_refresh_connection(void* agent) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (a->connected && has_hub(a) && a->last_error.empty()) return BBL::BAMBU_NETWORK_SUCCESS;
    return bambu_network_connect_server(agent);
}

PANDAR_ABI int bambu_network_start_subscribe(void* agent, std::string) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_stop_subscribe(void*, std::string) {
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_add_subscribe(void* agent, std::vector<std::string> dev_ids) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    {
        std::lock_guard<std::mutex> lock(a->status_mutex);
        for (const auto& dev_id : dev_ids) {
            if (!dev_id.empty()) a->cloud_subscribed_devices.insert(studio_dev_id(dev_id));
        }
    }
    emit_cloud_printer_connected_statuses(a, dev_ids);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_del_subscribe(void* agent, std::vector<std::string> dev_ids) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    for (const auto& dev_id : dev_ids) {
        const auto normalized_dev_id = studio_dev_id(dev_id);
        a->cloud_subscribed_devices.erase(normalized_dev_id);
        a->cloud_initialized_devices.erase(normalized_dev_id);
        a->cloud_connection_notifications.erase(normalized_dev_id);
    }
    return BBL::BAMBU_NETWORK_SUCCESS;
}
