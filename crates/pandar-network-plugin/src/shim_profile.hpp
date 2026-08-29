#pragma once

#include "shim_account_ffi.hpp"

namespace pandar::network_plugin {

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
