#pragma once

#include "shim_account_transaction.hpp"

namespace pandar::network_plugin {

bool try_no_auth_session(Agent* agent, bool initial_attempt) {
    if (!agent) return false;
    refresh_local_webserver_config(agent);
    bool committed = false;
    {
        std::lock_guard<std::mutex> request(agent->no_auth_refresh_mutex);
        auto lifecycle = pandar_plugin_account_no_auth_bootstrap(
            agent->printer_refresh_session,
            initial_attempt,
            studio_now_ms(),
            agent,
            with_current_account
        );
        const auto status = lifecycle.http.status;
        const auto http_code = lifecycle.http.http_code;
        auto body = body_from_result(lifecycle.http);
        trace_plugin_event(
            agent,
            "no-auth response status=" + std::to_string(status)
                + " http_code=" + std::to_string(http_code)
        );
        committed = lifecycle.account_event == kAccountEventLogin;
        if (committed) {
            enqueue_account_callback(agent, [agent] { dispatch_user_login(agent, true); });
        } else if (status != 0) {
            if (lifecycle.report_http_error != 0) {
                enqueue_account_callback(agent, [agent, http_code, body] {
                    dispatch_http_error(agent, http_code, body);
                });
            }
        }
    }
    drain_account_callbacks(agent);
    return committed;
}

} // namespace pandar::network_plugin
