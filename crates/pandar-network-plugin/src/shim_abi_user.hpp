#pragma once

#include "shim_firmware.hpp"
#include "shim_personal_presets.hpp"

namespace pandar::network_plugin {

} // namespace pandar::network_plugin

using namespace pandar::network_plugin;


PANDAR_IGNORE_CXX_LINKAGE_BEGIN

PANDAR_ABI std::string bambu_network_get_version() {
    return pandar_plugin_network_agent_version();
}

PANDAR_ABI std::string bambu_network_get_user_id(void* agent) {
    auto a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    if (!a) return {};
    std::lock_guard<std::recursive_mutex> refresh(a->printer_refresh_mutex);
    return a->user_id;
}

PANDAR_ABI std::string bambu_network_get_user_name(void* agent) {
    auto a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    if (!a) return {};
    std::lock_guard<std::recursive_mutex> refresh(a->printer_refresh_mutex);
    return a->user_name;
}

PANDAR_ABI std::string bambu_network_get_user_avatar(void* agent) {
    auto a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    if (!a) return {};
    std::lock_guard<std::recursive_mutex> refresh(a->printer_refresh_mutex);
    return a->avatar;
}

PANDAR_ABI std::string bambu_network_get_user_nickanme(void* agent) {
    auto a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    if (!a) return {};
    std::lock_guard<std::recursive_mutex> refresh(a->printer_refresh_mutex);
    return a->user_name;
}

PANDAR_ABI std::string bambu_network_build_login_cmd(void* agent) {
    auto a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    if (a) pandar_plugin_account_login_observation_clear(a->account_identity());
    return account_login_envelope(a, false);
}

PANDAR_ABI std::string bambu_network_build_logout_cmd(void* agent) {
    return account_login_envelope(as_agent(agent), true);
}

PANDAR_ABI std::string bambu_network_build_login_info(void* agent) {
    auto a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    if (a) pandar_plugin_account_login_observation_clear(a->account_identity());
    return account_login_envelope(a, false);
}

PANDAR_ABI std::string bambu_network_get_bambulab_host(void* agent) {
    auto a = as_agent(agent);
    if (!a) return {};
    auto result = rust_start_local_webserver(a);
    std::string body = body_from_result(result);
    if (static_cast<AccountPolicyAction>(pandar_plugin_account_response_action(result.status))
        != AccountPolicyAction::Apply) {
        return {};
    }
    auto parsed = pandar_plugin_account_local_base_url(
        reinterpret_cast<const uint8_t*>(body.data()), body.size()
    );
    auto base_url = body_from_result(parsed);
    if (static_cast<AccountPolicyAction>(pandar_plugin_account_response_action(parsed.status))
        == AccountPolicyAction::Apply) {
        return base_url;
    }
    return {};
}

PANDAR_ABI std::string bambu_network_get_user_selected_machine(void* agent) {
    auto a = as_agent(agent);
    if (!a) return {};
    auto selected = body_from_result(
        pandar_plugin_studio_selected(a->connection_session())
    );
    trace_plugin_event(a, std::string("get_user_selected_machine selected=") + selected);
    return selected;
}

PANDAR_ABI std::string bambu_network_get_studio_info_url(void* agent) {
    auto a = as_agent(agent);
    auto result = pandar_plugin_account_studio_info_url(
        a != nullptr,
        a && a->frontend_configured,
        reinterpret_cast<const uint8_t*>(a ? a->frontend_url.data() : nullptr),
        a ? a->frontend_url.size() : 0
    );
    auto body = body_from_result(result);
    const auto action = static_cast<AccountPolicyAction>(
        pandar_plugin_account_response_action(result.status)
    );
    return action == AccountPolicyAction::Apply ? body : std::string{};
}

PANDAR_ABI std::string bambu_network_request_setting_id(void* agent, std::string name, std::map<std::string, std::string>* values, unsigned int* http_code) {
    return preset_create(as_agent(agent), name, values, http_code);
}

PANDAR_IGNORE_CXX_LINKAGE_END

PANDAR_ABI bool bambu_network_check_debug_consistent(bool studio_debug) {
    return pandar_plugin_account_debug_consistent(studio_debug);
}

PANDAR_ABI void* bambu_network_create_agent(std::string log_dir) {
    auto* agent = new Agent(std::move(log_dir));
    RuntimeConfigCopy config;
    if (pandar_plugin_account_runtime_config(&config, copy_runtime_config) != 0) {
        delete agent;
        return nullptr;
    }
    agent->hub_url = std::move(config.hub_url);
    agent->frontend_url = std::move(config.frontend_url);
    agent->hub_configured = config.hub_configured;
    agent->frontend_configured = config.frontend_configured;
    agent->plugin_core = pandar_plugin_core_create(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size()
    );
    if (!agent->plugin_core) {
        delete agent;
        return nullptr;
    }
    pandar_plugin_connection_set_dispatch_waker(
        agent->connection_session(), agent, shim_wake_status_dispatcher
    );
    start_firmware_dispatcher(agent);
    start_status_heartbeat(agent);
    start_model_task_worker(agent);
    return agent;
}

void reclaim_agent(Agent* a) {
    stop_model_task_worker(a);
    stop_status_heartbeat(a);
    stop_firmware_dispatcher(a);
    a->lifetime.wait_for_leases();
    if (a) {
        pandar_plugin_studio_set_listener(
            a->connection_session(), kStudioCloudListener, false
        );
        pandar_plugin_studio_set_listener(
            a->connection_session(), kStudioLocalListener, false
        );
        pandar_plugin_studio_set_listener(
            a->connection_session(), kStudioPrinterConnectedListener, false
        );
        pandar_plugin_studio_set_listener(
            a->connection_session(), kStudioLocalConnectedListener, false
        );
        pandar_plugin_core_destroy(a->plugin_core);
        a->plugin_core = nullptr;
        std::lock_guard<std::mutex> lock(a->status_mutex);
        a->on_message = {};
        a->on_local_message = {};
        a->on_printer_connected = {};
        a->on_server_connected = {};
        a->on_local_connect = {};
        a->on_ssdp_message = {};
        a->on_user_login = {};
        a->on_http_error = {};
        a->get_country_code = {};
        a->on_subscribe_failure = {};
        a->on_user_message = {};
        a->queue_on_main = {};
        a->on_server_error = {};
    }
    delete a;
}

void pandar::network_plugin::finalize_agent_destroy(Agent* a) {
    if (!a || !a->lifetime.begin_finalizer()) return;
    reclaim_agent(a);
}

PANDAR_ABI int bambu_network_destroy_agent(void* agent) {
    auto* a = reinterpret_cast<Agent*>(agent);
    if (!a) return BBL::BAMBU_NETWORK_SUCCESS;
    if (!AgentCallLease::held_by_current_thread(a)) {
        if (a->lifetime.request_destroy_and_begin_finalizer()) reclaim_agent(a);
        return BBL::BAMBU_NETWORK_SUCCESS;
    }
    a->lifetime.request_destroy();
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_init_log(void* agent) {
    return studio_disposition(as_agent(agent), StudioDisposition::InitLog);
}

PANDAR_ABI int bambu_network_set_config_dir(void* agent, std::string config_dir) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    {
        std::lock_guard<std::recursive_mutex> account(a->account_mutex);
        {
            std::lock_guard<std::mutex> lock(a->trace_mutex);
            a->config_dir = std::move(config_dir);
        }
        a->account_config_epoch.fetch_add(1, std::memory_order_release);
        pandar_plugin_personal_preset_reset(a->account_identity());
    }
    auto lifecycle = pandar_plugin_account_load_persisted(a, with_current_account);
    body_from_result(lifecycle.http);
    return pandar_plugin_account_response_status(lifecycle.http.status);
}

PANDAR_ABI int bambu_network_set_cert_file(void* agent, std::string folder, std::string filename) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    (void)folder;
    (void)filename;
    return studio_disposition(a, StudioDisposition::SetCert);
}

PANDAR_ABI int bambu_network_set_country_code(void* agent, std::string country_code) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    a->country_code = std::move(country_code);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_start(void* agent) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    AgentCallLease lease(a);
    if (!lease) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    auto result = rust_start_local_webserver(a);
    body_from_result(result);
    if (static_cast<AccountPolicyAction>(pandar_plugin_account_response_action(result.status))
        != AccountPolicyAction::Apply) {
        return pandar_plugin_account_response_status(result.status);
    }
    try_no_auth_session(a, true);
    return pandar_plugin_account_response_status(result.status);
}

#define PANDAR_CALLBACK_SETTER(name, type, field) \
    PANDAR_ABI int name(void* agent, BBL::type callback) { \
        auto a = as_agent(agent); \
        if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE; \
        std::lock_guard<std::mutex> lock(a->status_mutex); \
        a->field = std::move(callback); \
        return BBL::BAMBU_NETWORK_SUCCESS; \
    }

PANDAR_CALLBACK_SETTER(bambu_network_set_on_ssdp_msg_fn, OnMsgArrivedFn, on_ssdp_message)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_user_login_fn, OnUserLoginFn, on_user_login)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_http_error_fn, OnHttpErrorFn, on_http_error)
PANDAR_CALLBACK_SETTER(bambu_network_set_get_country_code_fn, GetCountryCodeFn, get_country_code)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_subscribe_failure_fn, GetSubscribeFailureFn, on_subscribe_failure)
PANDAR_CALLBACK_SETTER(bambu_network_set_on_user_message_fn, OnMessageFn, on_user_message)
PANDAR_CALLBACK_SETTER(bambu_network_set_queue_on_main_fn, QueueOnMainFn, queue_on_main)
PANDAR_CALLBACK_SETTER(bambu_network_set_server_callback, OnServerErrFn, on_server_error)

#undef PANDAR_CALLBACK_SETTER

PANDAR_ABI int bambu_network_set_on_server_connected_fn(void* agent, BBL::OnServerConnectedFn callback) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    {
        std::lock_guard<std::mutex> lock(a->status_mutex);
        a->on_server_connected = std::move(callback);
    }
    shim_wake_status_dispatcher(a);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_on_message_fn(void* agent, BBL::OnMessageFn callback) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    if (pandar_plugin_studio_set_listener(
            a->connection_session(),
            kStudioCloudListener,
            static_cast<bool>(callback)
        ) != 0) return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    a->on_message = std::move(callback);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_on_local_message_fn(void* agent, BBL::OnMessageFn callback) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    if (pandar_plugin_studio_set_listener(
            a->connection_session(),
            kStudioLocalListener,
            static_cast<bool>(callback)
        ) != 0) return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    a->on_local_message = std::move(callback);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_on_printer_connected_fn(void* agent, BBL::OnPrinterConnectedFn callback) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    if (pandar_plugin_studio_set_listener(
            a->connection_session(),
            kStudioPrinterConnectedListener,
            static_cast<bool>(callback)
        ) != 0) return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    a->on_printer_connected = std::move(callback);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_on_local_connect_fn(void* agent, BBL::OnLocalConnectedFn callback) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    if (pandar_plugin_studio_set_listener(
            a->connection_session(),
            kStudioLocalConnectedListener,
            static_cast<bool>(callback)
        ) != 0) return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    a->on_local_connect = std::move(callback);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_connect_server(void* agent) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    refresh_local_webserver_config(a);
    {
        std::lock_guard<std::recursive_mutex> refresh(a->printer_refresh_mutex);
        pandar_plugin_connection_refresh(a->connection_session());
    }
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI bool bambu_network_is_server_connected(void* agent) {
    auto a = as_agent(agent);
    if (!a) return false;
    std::lock_guard<std::recursive_mutex> refresh(a->printer_refresh_mutex);
    return pandar_plugin_connection_is_connected(a->connection_session()) != 0;
}

PANDAR_ABI int bambu_network_refresh_connection(void* agent) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    return bambu_network_connect_server(agent);
}

PANDAR_ABI int bambu_network_start_subscribe(void* agent, std::string) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    return studio_disposition(a, StudioDisposition::StartSubscribe);
}

PANDAR_ABI int bambu_network_stop_subscribe(void* agent, std::string) {
    return studio_disposition(as_agent(agent), StudioDisposition::StopSubscribe);
}

PANDAR_ABI int bambu_network_add_subscribe(void* agent, std::vector<std::string> dev_ids) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    for (const auto& dev_id : dev_ids) {
        if (dev_id.empty() || pandar_plugin_studio_add_subscription(
                a->connection_session(),
                reinterpret_cast<const uint8_t*>(dev_id.data()), dev_id.size()
            ) != 0) return BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED;
    }
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_del_subscribe(void* agent, std::vector<std::string> dev_ids) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    for (const auto& dev_id : dev_ids) {
        if (dev_id.empty() || pandar_plugin_studio_del_subscription(
                a->connection_session(),
                reinterpret_cast<const uint8_t*>(dev_id.data()), dev_id.size()
            ) != 0) return BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED;
    }
    return BBL::BAMBU_NETWORK_SUCCESS;
}
