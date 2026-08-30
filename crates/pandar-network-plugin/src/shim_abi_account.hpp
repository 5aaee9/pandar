#pragma once

#include "shim_firmware.hpp"

using namespace pandar::network_plugin;

PANDAR_ABI int bambu_network_change_user(void* agent, std::string user_info) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    AgentCallLease lease(a);
    if (!lease) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    auto lifecycle = pandar_plugin_account_change_user(
        a->connection_session(),
        a->account_identity(),
        reinterpret_cast<const uint8_t*>(user_info.data()), user_info.size(),
        a,
        with_current_account
    );
    const auto status = lifecycle.http.status;
    body_from_result(lifecycle.http);
    drain_account_callbacks(a);
    return pandar_plugin_account_response_status(status);
}

PANDAR_ABI bool bambu_network_is_user_login(void* agent) {
    auto a = as_agent(agent);
    if (!a) return false;
    AgentCallLease lease(a);
    if (!lease) return false;
    refresh_local_webserver_config(a);
    std::lock_guard<std::recursive_mutex> refresh(a->printer_refresh_mutex);
    return pandar_plugin_account_observe_login(
        a->account_identity(),
        studio_session_state(a).account_epoch,
        reinterpret_cast<const uint8_t*>(a->token.data()),
        a->token.size()
    );
}

PANDAR_ABI int bambu_network_user_logout(void* agent, bool request) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    AgentCallLease lease(a);
    if (!lease) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    auto lifecycle = pandar_plugin_account_logout(
        a->connection_session(),
        a->account_identity(),
        request,
        a,
        with_current_account
    );
    const auto status = lifecycle.http.status;
    body_from_result(lifecycle.http);
    drain_account_callbacks(a);
    return pandar_plugin_account_response_status(status);
}

PANDAR_ABI int bambu_network_get_my_profile(
    void* agent,
    std::string token,
    unsigned int* http_code,
    std::string* http_body
) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    AgentCallLease lease(a);
    if (!lease) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    auto lifecycle = pandar_plugin_account_profile(
        reinterpret_cast<const uint8_t*>(token.data()), token.size(),
        a,
        with_current_account
    );
    const auto status = lifecycle.http.status;
    const auto code = lifecycle.http.http_code;
    auto body = body_from_result(lifecycle.http);
    if (http_code) *http_code = code;
    if (http_body) *http_body = body;
    return pandar_plugin_account_response_status(status);
}

PANDAR_ABI int bambu_network_get_my_token(
    void* agent,
    std::string ticket,
    unsigned int* http_code,
    std::string* http_body
) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    AgentCallLease lease(a);
    if (!lease) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    refresh_local_webserver_config(a);
    auto lifecycle = pandar_plugin_account_exchange_ticket(
        reinterpret_cast<const uint8_t*>(ticket.data()), ticket.size(),
        a,
        with_current_account
    );
    const auto status = lifecycle.http.status;
    const auto code = lifecycle.http.http_code;
    auto body = body_from_result(lifecycle.http);
    if (http_code) *http_code = code;
    if (http_body) *http_body = body;
    return pandar_plugin_account_response_status(status);
}
