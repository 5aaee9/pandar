#pragma once

#include "shim_account_ffi.hpp"

namespace pandar::network_plugin {

struct AccountCopy {
    std::string token;
    std::string user_id;
    std::string user_name;
    std::string avatar;
    std::string profile_json;
    std::int32_t session_kind = 0;
};

extern "C" void copy_account(
    void* context,
    const uint8_t* token, std::size_t token_len,
    const uint8_t* user_id, std::size_t user_id_len,
    const uint8_t* user_name, std::size_t user_name_len,
    const uint8_t* avatar, std::size_t avatar_len,
    const uint8_t* profile, std::size_t profile_len,
    std::int32_t session_kind
) {
    auto& copy = *static_cast<AccountCopy*>(context);
    copy.token.assign(reinterpret_cast<const char*>(token), token_len);
    copy.user_id.assign(reinterpret_cast<const char*>(user_id), user_id_len);
    copy.user_name.assign(reinterpret_cast<const char*>(user_name), user_name_len);
    copy.avatar.assign(reinterpret_cast<const char*>(avatar), avatar_len);
    copy.profile_json.assign(reinterpret_cast<const char*>(profile), profile_len);
    copy.session_kind = session_kind;
}

struct RuntimeConfigCopy {
    std::string hub_url;
    std::string frontend_url;
    bool hub_configured = false;
    bool frontend_configured = false;
};

extern "C" void copy_runtime_config(
    void* context,
    const uint8_t* hub_url, std::size_t hub_url_len, bool hub_configured,
    const uint8_t* frontend_url, std::size_t frontend_url_len, bool frontend_configured
) {
    auto& copy = *static_cast<RuntimeConfigCopy*>(context);
    copy.hub_url.assign(reinterpret_cast<const char*>(hub_url), hub_url_len);
    copy.frontend_url.assign(reinterpret_cast<const char*>(frontend_url), frontend_url_len);
    copy.hub_configured = hub_configured;
    copy.frontend_configured = frontend_configured;
}

std::string body_from_result(PluginHttpResult result) {
    std::string body;
    if (result.body_ptr && result.body_len > 0) {
        body.assign(reinterpret_cast<char*>(result.body_ptr), result.body_len);
    }
    pandar_plugin_free_with_capacity(result.body_ptr, result.body_len, result.body_cap);
    return body;
}

void apply_account_copy_under_refresh(Agent* agent, AccountCopy copy) {
    agent->token = std::move(copy.token);
    agent->user_id = std::move(copy.user_id);
    agent->user_name = std::move(copy.user_name);
    agent->avatar = std::move(copy.avatar);
    agent->profile_json = std::move(copy.profile_json);
    agent->account_session_kind = copy.session_kind;
    sync_printer_refresh_session(agent);
}

std::string account_login_envelope(const Agent* agent, bool logout) {
    std::string token, user_id, user_name, avatar;
    if (agent) {
        std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
        token = agent->token;
        user_id = agent->user_id;
        user_name = agent->user_name;
        avatar = agent->avatar;
    }
    auto result = pandar_plugin_account_login_envelope(
        logout,
        reinterpret_cast<const uint8_t*>(token.data()), token.size(),
        reinterpret_cast<const uint8_t*>(user_id.data()), user_id.size(),
        reinterpret_cast<const uint8_t*>(user_name.data()), user_name.size(),
        reinterpret_cast<const uint8_t*>(avatar.data()), avatar.size()
    );
    return body_from_result(result);
}

int studio_disposition(
    Agent* agent,
    StudioDisposition operation,
    std::string* body = nullptr,
    unsigned int* http_code = nullptr
) {
    auto result = pandar_plugin_studio_disposition(static_cast<uint32_t>(operation), agent != nullptr);
    auto message = body_from_result(result);
    if (http_code) *http_code = result.http_code;
    if (body) *body = std::move(message);
    return result.status;
}

void dispatch_user_login(Agent* agent, bool login) {
    if (!agent) return;
    BBL::OnUserLoginFn callback;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        callback = agent->on_user_login;
    }
    std::lock_guard<std::recursive_timed_mutex> gate(agent->callback_mutex);
    if (callback) callback(login ? 1 : 0, login);
}

void dispatch_http_error(Agent* agent, unsigned code, const std::string& body) {
    if (!agent) return;
    BBL::OnHttpErrorFn callback;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        callback = agent->on_http_error;
    }
    std::lock_guard<std::recursive_timed_mutex> gate(agent->callback_mutex);
    if (callback) callback(code, body);
}

} // namespace pandar::network_plugin
