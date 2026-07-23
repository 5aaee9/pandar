#pragma once

namespace pandar::network_plugin {

constexpr std::int32_t kAccountMutationReplace = 1;
constexpr std::int32_t kAccountMutationClear = 2;
constexpr std::int32_t kAccountMutationHttpError = 3;
constexpr std::int32_t kAccountMutationLogin = 4;
constexpr std::int32_t kAccountMutationRuntimeHub = 6;
constexpr std::int32_t kAccountMutationFirmwareFence = 7;
constexpr std::int32_t kAccountMutationRestoreFailure = 8;
constexpr std::int32_t kAccountNotificationLogout = 2;
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

extern "C" std::int32_t with_current_account(
    void* opaque,
    void* rust_context,
    PluginAccountTransaction transaction
) {
    auto* agent = static_cast<Agent*>(opaque);
    if (!agent || !transaction) return 1;
    std::int32_t status = 1;
    bool drain_callbacks = false;
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
        if (status == 0 && (mutation.action == kAccountMutationReplace ||
                            mutation.action == kAccountMutationLogin)) {
            AccountCopy copy{
                account_string(mutation.token),
                account_string(mutation.user_id),
                account_string(mutation.user_name),
                account_string(mutation.avatar),
                account_string(mutation.profile_json),
                mutation.session_kind,
            };
            LocalLostDelivery lost;
            if (mutation.action == kAccountMutationLogin) {
                lost = reset_account_printer_state(agent);
            }
            apply_account_copy_under_refresh(agent, std::move(copy));
            if (mutation.action == kAccountMutationLogin) {
                auto transition = finish_account_printer_transition(agent, lost);
                enqueue_account_callback(
                    agent,
                    [agent, transition = std::move(transition)]() mutable {
                        transition();
                        dispatch_user_login(agent, true);
                    }
                );
                drain_callbacks = true;
            }
        } else if (status == 0 && mutation.action == kAccountMutationClear) {
            const bool notify_logout =
                mutation.notification == kAccountNotificationLogout;
            const auto lost = clear_login_state(agent);
            auto transition = finish_account_printer_transition(agent, lost);
            const auto transition_current = account_transition_current(agent, lost, {}, true);
            enqueue_account_callback(
                agent,
                [agent, transition = std::move(transition), transition_current, notify_logout]() mutable {
                    transition();
                    if (notify_logout && transition_current) dispatch_user_login(agent, false);
                }
            );
            drain_callbacks = true;
        } else if (status == 0 && mutation.action == kAccountMutationHttpError) {
            auto body = account_string(mutation.error_body);
            enqueue_account_callback(
                agent,
                [agent, code = mutation.http_code, body = std::move(body)] {
                    dispatch_http_error(agent, code, body);
                }
            );
            drain_callbacks = true;
        } else if (status == 0 && mutation.action == kAccountMutationRuntimeHub) {
            const auto lost = clear_login_state(agent, false);
            agent->hub_url = account_string(mutation.hub_url);
            sync_printer_refresh_session(agent);
            auto transition = finish_account_printer_transition(agent, lost);
            enqueue_account_callback(agent, std::move(transition));
            drain_callbacks = true;
        } else if (status == 0 && mutation.action == kAccountMutationFirmwareFence) {
            invalidate_firmware_account_session(agent);
        } else if (status == 0 && mutation.action == kAccountMutationRestoreFailure) {
            AccountCopy copy{
                account_string(mutation.token),
                account_string(mutation.user_id),
                account_string(mutation.user_name),
                account_string(mutation.avatar),
                account_string(mutation.profile_json),
                mutation.session_kind,
            };
            auto body = account_string(mutation.error_body);
            const auto code = mutation.http_code;
            const auto lost = reset_account_printer_state(agent);
            apply_account_copy_under_refresh(agent, std::move(copy));
            auto transition = finish_account_printer_transition(agent, lost);
            const auto restored = studio_session_state(agent);
            const auto restored_config_epoch =
                agent->account_config_epoch.load(std::memory_order_acquire);
            const auto restored_hub = agent->hub_url;
            const auto restored_token = agent->token;
            const auto restored_kind = agent->account_session_kind;
            enqueue_account_callback(
                agent,
                [agent, transition = std::move(transition), restored,
                 restored_config_epoch, restored_hub, restored_token, restored_kind,
                 code, body = std::move(body)]() mutable {
                    transition();
                    const auto current = [&] {
                        std::lock_guard<std::recursive_mutex> account(agent->account_mutex);
                        std::lock_guard<std::recursive_mutex> refresh(agent->printer_refresh_mutex);
                        const auto state = studio_session_state(agent);
                        return !state.account_transition_pending &&
                            state.account_epoch == restored.account_epoch &&
                            agent->account_config_epoch.load(std::memory_order_acquire) ==
                                restored_config_epoch &&
                            agent->hub_url == restored_hub && agent->token == restored_token &&
                            agent->account_session_kind == restored_kind;
                    };
                    if (!current()) return;
                    dispatch_user_login(agent, true);
                    if (!body.empty() && current()) dispatch_http_error(agent, code, body);
                }
            );
        }
    }
    if (drain_callbacks) drain_account_callbacks(agent);
    return status;
}

} // namespace pandar::network_plugin
