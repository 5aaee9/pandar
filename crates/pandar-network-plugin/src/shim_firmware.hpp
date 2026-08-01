#pragma once

#include "shim_firmware_request.hpp"
#include "shim_status.hpp"
#include "shim_request_snapshot.hpp"
#include "shim_session_sync.hpp"

namespace pandar::network_plugin {

std::string string_from_firmware_allocation(uint8_t* ptr, std::size_t len, std::size_t cap) {
    std::string value;
    if (ptr && len > 0) value.assign(reinterpret_cast<char*>(ptr), len);
    pandar_plugin_free_with_capacity(ptr, len, cap);
    return value;
}

FirmwareObservationTicket begin_firmware_observation(Agent* agent) {
    if (!agent) return {};
    std::lock_guard<std::recursive_mutex> transition(agent->firmware_transition_mutex);
    return {
        agent->firmware_generation,
        ++agent->firmware_observation_sequence,
    };
}

void start_firmware_dispatcher(Agent* agent) {
    if (!agent || agent->firmware_thread.joinable()) return;
    agent->firmware_thread_stop = false;
    agent->firmware_thread = std::thread([agent] {
        while (!agent->firmware_thread_stop.load()) {
            if (agent->firmware_transition_pending.load()) {
                std::this_thread::yield();
                continue;
            }
            PluginFirmwareCallbackResult result{};
            std::uint64_t callback_generation = 0;
            {
                std::lock_guard<std::recursive_mutex> transition(agent->firmware_transition_mutex);
                result = pandar_plugin_firmware_next_callback(agent->firmware_session, 25);
                callback_generation = agent->firmware_generation;
            }
            if (result.status == 0) {
                auto dev_id = string_from_firmware_allocation(
                    result.dev_id_ptr,
                    result.dev_id_len,
                    result.dev_id_cap
                );
                auto message = string_from_firmware_allocation(
                    result.message_ptr,
                    result.message_len,
                    result.message_cap
                );
                const auto tunnel = result.tunnel == 0
                    ? MessageTunnel::Cloud
                    : MessageTunnel::Local;
                const auto callback_deadline = std::chrono::steady_clock::time_point(
                    std::chrono::duration_cast<std::chrono::steady_clock::duration>(
                        std::chrono::nanoseconds(
                            static_cast<std::chrono::nanoseconds::rep>(result.origin_tick)
                        )
                    )
                ) + std::chrono::seconds(2);
                std::unique_lock<std::recursive_timed_mutex> callback_gate(
                    agent->callback_mutex,
                    std::defer_lock
                );
                if (!callback_gate.try_lock_until(callback_deadline)) continue;
                if (std::chrono::steady_clock::now() >= callback_deadline) {
                    continue;
                }
                if (std::chrono::steady_clock::now() < callback_deadline) {
                    deliver_printer_message(
                        agent,
                        tunnel,
                        dev_id,
                        message,
                        0,
                        result.local_generation,
                        false,
                        result.cache_generation,
                        callback_generation
                    );
                }
            }
            if (result.status == 0) {
                std::this_thread::yield();
            } else {
                std::this_thread::sleep_for(std::chrono::milliseconds(1));
            }
        }
    });
}

void stop_firmware_dispatcher(Agent* agent) {
    if (!agent) return;
    agent->firmware_thread_stop = true;
    pandar_plugin_firmware_stop(agent->firmware_session);
    if (agent->firmware_thread.joinable()) agent->firmware_thread.join();
}

PluginHttpResult rust_start_local_webserver(const Agent* agent) {
    return pandar_plugin_start_local_webserver(
        reinterpret_cast<const uint8_t*>(agent->frontend_url.data()),
        agent->frontend_url.size(),
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        agent->frontend_configured,
        agent->hub_configured
    );
}

void refresh_local_webserver_config(Agent* agent) {
    auto lifecycle = pandar_plugin_account_refresh_runtime(agent, with_current_account);
    body_from_result(lifecycle.http);
}

PluginHttpResult rust_operation_json_from_gcode(const std::string& message) {
    return pandar_plugin_operation_json_from_gcode(
        reinterpret_cast<const uint8_t*>(message.data()),
        message.size()
    );
}

PluginHttpResult rust_submit_printer_operation(
    const PrinterRequestSnapshot& snapshot,
    const std::string& operation_json
) {
    return pandar_plugin_submit_printer_operation(
        reinterpret_cast<const uint8_t*>(snapshot.hub_url.data()),
        snapshot.hub_url.size(),
        reinterpret_cast<const uint8_t*>(snapshot.token.data()),
        snapshot.token.size(),
        reinterpret_cast<const uint8_t*>(snapshot.printer_id.data()),
        snapshot.printer_id.size(),
        reinterpret_cast<const uint8_t*>(operation_json.data()),
        operation_json.size()
    );
}

PluginHttpResult rust_submit_h2c_auto_nozzle_mapping(
    const PrinterRequestSnapshot& snapshot,
    const std::string& request_json
) {
    return pandar_plugin_submit_h2c_auto_nozzle_mapping(
        reinterpret_cast<const uint8_t*>(snapshot.hub_url.data()),
        snapshot.hub_url.size(),
        reinterpret_cast<const uint8_t*>(snapshot.token.data()),
        snapshot.token.size(),
        reinterpret_cast<const uint8_t*>(snapshot.printer_id.data()),
        snapshot.printer_id.size(),
        reinterpret_cast<const uint8_t*>(request_json.data()),
        request_json.size()
    );
}

int submit_printer_operation_json(Agent* agent, std::string dev_id, const std::string& operation_json) {
    refresh_local_webserver_config(agent);
    const auto snapshot = printer_request_snapshot(agent, dev_id);
    auto admission = pandar_plugin_studio_request_admitted(
        snapshot.printer_authorized, snapshot.account_transition_pending
    );
    if (admission.status != 0) {
        const auto status = admission.status;
        body_from_result(admission);
        return status;
    }
    body_from_result(admission);
    auto upstream = rust_submit_printer_operation(snapshot, operation_json);
    const auto upstream_status = upstream.status;
    const auto upstream_http_code = upstream.http_code;
    std::string body = body_from_result(upstream);
    auto result = pandar_plugin_studio_printer_operation_result(
        upstream_status,
        upstream_http_code,
        reinterpret_cast<const uint8_t*>(body.data()), body.size(),
        printer_request_snapshot_current(agent, snapshot)
    );
    body_from_result(result);
    return result.status;
}

bool submit_h2c_auto_nozzle_mapping(
    Agent* agent,
    const std::string& dev_id,
    const std::string& request_json,
    MessageTunnel tunnel,
    std::uint64_t local_generation
) {
    refresh_local_webserver_config(agent);
    const auto snapshot = printer_request_snapshot(agent, dev_id);
    auto admission = pandar_plugin_studio_request_admitted(
        snapshot.printer_authorized, snapshot.account_transition_pending
    );
    if (admission.status != 0) {
        body_from_result(admission);
        return false;
    }
    body_from_result(admission);
    auto upstream = rust_submit_h2c_auto_nozzle_mapping(snapshot, request_json);
    const auto upstream_status = upstream.status;
    const auto upstream_http_code = upstream.http_code;
    std::string body = body_from_result(upstream);
    auto result = pandar_plugin_studio_printer_operation_result(
        upstream_status,
        upstream_http_code,
        reinterpret_cast<const uint8_t*>(body.data()), body.size(),
        printer_request_snapshot_current(agent, snapshot)
    );
    const auto result_status = result.status;
    body = body_from_result(result);
    if (result_status != 0) return false;
    return deliver_printer_message(
        agent,
        tunnel,
        dev_id,
        body,
        snapshot.account_epoch,
        local_generation,
        false,
        snapshot.cache_generation,
        snapshot.firmware_generation
    );
}

struct FirmwareSendAttempt {
    bool handled;
    int result;
    std::uint64_t callback_token;
    std::uint64_t local_generation;
    std::uint64_t cache_generation;
};

FirmwareSendAttempt begin_firmware_send(
    Agent* agent,
    const std::string& dev_id,
    const std::string& message,
    MessageTunnel tunnel,
    std::uint64_t local_generation
) {
    const auto normalized_dev_id = studio_dev_id(dev_id);
    const auto snapshot = printer_request_snapshot(agent, normalized_dev_id);
    const auto cache_generation = snapshot.cache_generation;
    auto admission = pandar_plugin_studio_request_admitted(
        snapshot.printer_authorized, snapshot.account_transition_pending
    );
    if (admission.status != 0) {
        const auto status = admission.status;
        body_from_result(admission);
        return {true, status, 0, local_generation, cache_generation};
    }
    body_from_result(admission);
    std::uint64_t callback_token = 0;
    auto result = firmware_send_from_snapshot(
        pandar_plugin_firmware_send,
        agent->firmware_session,
        normalized_dev_id,
        message,
        tunnel == MessageTunnel::Cloud ? 0 : 1,
        &callback_token,
        snapshot
    );
    body_from_result(result);
    if (result.status == 2) {
        return {false, BBL::BAMBU_NETWORK_SUCCESS, 0, local_generation, cache_generation};
    }
    if (result.status != 0) {
        return {
            true,
            BBL::BAMBU_NETWORK_ERR_INVALID_RESULT,
            0,
            local_generation,
            cache_generation,
        };
    }
    return {
        true,
        BBL::BAMBU_NETWORK_SUCCESS,
        callback_token,
        local_generation,
        cache_generation,
    };
}

int finish_firmware_send(Agent* agent, FirmwareSendAttempt attempt) {
    if (attempt.callback_token != 0) {
        const auto tick = std::chrono::duration_cast<std::chrono::nanoseconds>(
            std::chrono::steady_clock::now().time_since_epoch()
        ).count();
        pandar_plugin_firmware_return_handoff(
            agent->firmware_session,
            attempt.callback_token,
            static_cast<std::uint64_t>(tick),
            attempt.local_generation,
            attempt.cache_generation
        );
    }
    return attempt.result;
}

} // namespace pandar::network_plugin

#include "shim_no_auth.hpp"
