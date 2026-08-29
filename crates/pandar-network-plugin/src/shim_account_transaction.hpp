#pragma once

#include "shim_dispatch.hpp"
#include "shim_personal_preset_types.hpp"

namespace pandar::network_plugin {

constexpr std::int32_t kAccountEventLogin = 1;

inline PluginAccountBytes account_bytes(const std::string& value) {
    return {
        reinterpret_cast<const std::uint8_t*>(value.data()),
        value.size(),
    };
}

inline std::string account_string(PluginAccountBytes value) {
    if (value.len == 0) return {};
    return std::string(reinterpret_cast<const char*>(value.ptr), value.len);
}

extern "C" void shim_account_replace(
    void* opaque,
    PluginAccountBytes token,
    PluginAccountBytes user_id,
    PluginAccountBytes user_name,
    PluginAccountBytes avatar,
    PluginAccountBytes profile_json,
    PluginAccountBytes tenant_id,
    std::int32_t session_kind
) {
    auto* agent = static_cast<Agent*>(opaque);
    agent->token = account_string(token);
    agent->user_id = account_string(user_id);
    agent->user_name = account_string(user_name);
    agent->avatar = account_string(avatar);
    agent->profile_json = account_string(profile_json);
    agent->tenant_id = account_string(tenant_id);
    agent->account_session_kind = session_kind;
}

extern "C" void shim_account_clear(void* opaque) {
    auto* agent = static_cast<Agent*>(opaque);
    agent->token.clear();
    agent->user_id.clear();
    agent->user_name.clear();
    agent->avatar.clear();
    agent->profile_json.clear();
    agent->tenant_id.clear();
    agent->account_session_kind = 0;
}

extern "C" void shim_account_set_hub_url(void* opaque, PluginAccountBytes hub_url) {
    static_cast<Agent*>(opaque)->hub_url = account_string(hub_url);
}

extern "C" void shim_account_invoke_user_login(
    void* opaque,
    std::int32_t status,
    bool login
) {
    auto* agent = static_cast<Agent*>(opaque);
    BBL::OnUserLoginFn callback;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        callback = agent->on_user_login;
    }
    std::lock_guard<std::recursive_timed_mutex> gate(agent->callback_mutex);
    if (callback) callback(status, login);
}

extern "C" void shim_account_invoke_http_error(
    void* opaque,
    std::uint32_t code,
    PluginAccountBytes body
) {
    auto* agent = static_cast<Agent*>(opaque);
    BBL::OnHttpErrorFn callback;
    {
        std::lock_guard<std::mutex> lock(agent->status_mutex);
        callback = agent->on_http_error;
    }
    std::lock_guard<std::recursive_timed_mutex> gate(agent->callback_mutex);
    if (callback) callback(code, account_string(body));
}

extern "C" void shim_account_reset_personal_presets(void* opaque) {
    auto* agent = static_cast<Agent*>(opaque);
    pandar_plugin_personal_preset_reset(agent->account_identity);
}

const PluginAccountSessionBridge kAccountSessionBridge{
    shim_account_replace,
    shim_account_clear,
    shim_account_set_hub_url,
    shim_account_invoke_user_login,
    shim_account_invoke_http_error,
    shim_account_reset_personal_presets,
};

void drain_account_callbacks(Agent* agent) {
    if (!agent) return;
    pandar_plugin_account_session_drain(
        agent->account_session,
        agent->printer_refresh_session,
        &kDispatchBridge,
        &kAccountSessionBridge,
        agent,
        agent,
        with_current_account
    );
}

extern "C" std::int32_t with_current_account(
    void* opaque,
    void* rust_context,
    PluginAccountTransaction transaction
) {
    auto* agent = static_cast<Agent*>(opaque);
    if (!agent || !transaction) return 1;
    std::int32_t status = 1;
    {
        std::lock_guard<std::recursive_mutex> account(agent->account_mutex);
        std::string config_dir;
        {
            std::lock_guard<std::mutex> trace(agent->trace_mutex);
            config_dir = agent->config_dir;
        }
        std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
        const auto state = studio_session_state(agent);
        const PluginAccountView current{
            account_bytes(config_dir),
            account_bytes(agent->hub_url),
            account_bytes(agent->token),
            account_bytes(agent->user_id),
            account_bytes(agent->user_name),
            account_bytes(agent->avatar),
            account_bytes(agent->profile_json),
            state.account_epoch,
            agent->account_config_epoch.load(std::memory_order_acquire),
            agent->account_session_kind,
            state.account_transition_pending,
        };
        PluginAccountMutation mutation{};
        status = transaction(rust_context, &current, &mutation);
        if (status == 0) {
            status = pandar_plugin_account_session_apply(
                agent->account_session,
                agent->printer_refresh_session,
                agent->firmware_session,
                &kAccountSessionBridge,
                agent,
                &current,
                &mutation
            );
        }
    }
    drain_account_callbacks(agent);
    return status;
}

} // namespace pandar::network_plugin
