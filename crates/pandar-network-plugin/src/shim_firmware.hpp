#pragma once

#include "shim_status.hpp"

namespace pandar::network_plugin {

std::string string_from_firmware_allocation(uint8_t* ptr, std::size_t len, std::size_t cap) {
    std::string value;
    if (ptr && len > 0) value.assign(reinterpret_cast<char*>(ptr), len);
    pandar_plugin_free_with_capacity(ptr, len, cap);
    return value;
}

BBL::OnMessageFn message_callback_for(Agent* agent, MessageTunnel tunnel) {
    std::lock_guard<std::mutex> lock(agent->status_mutex);
    return tunnel == MessageTunnel::Cloud ? agent->on_message : agent->on_local_message;
}

void invoke_message_callback(
    Agent* agent,
    const BBL::OnMessageFn& callback,
    const std::string& dev_id,
    const std::string& body
) {
    if (!callback) return;
    std::lock_guard<std::timed_mutex> lock(agent->callback_mutex);
    callback(dev_id, body);
}

void invoke_message_callback(
    Agent* agent,
    MessageTunnel tunnel,
    const std::string& dev_id,
    const std::string& body
) {
    invoke_message_callback(agent, message_callback_for(agent, tunnel), dev_id, body);
}

void sync_firmware_session(Agent* agent) {
    if (!agent || !agent->firmware_session) return;
    agent->firmware_transition_pending = true;
    std::lock_guard<std::recursive_mutex> transition(agent->firmware_transition_mutex);
    if (agent->firmware_hub_url == agent->hub_url && agent->firmware_token == agent->token) {
        agent->firmware_transition_pending = false;
        return;
    }
    const auto previous_generation = agent->firmware_generation;
    pandar_plugin_firmware_cancel_generation(agent->firmware_session, previous_generation);
    ++agent->firmware_generation;
    pandar_plugin_firmware_session_update(
        agent->firmware_session,
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size(),
        agent->firmware_generation
    );
    agent->firmware_hub_url = agent->hub_url;
    agent->firmware_token = agent->token;
    agent->firmware_transition_pending = false;
}

void sync_printer_refresh_session(Agent* agent) {
    if (!agent || !agent->printer_refresh_session) return;
    pandar_plugin_printer_refresh_session_update(
        agent->printer_refresh_session,
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size()
    );
    sync_firmware_session(agent);
}

FirmwareObservationTicket begin_firmware_observation(Agent* agent) {
    if (!agent) return {};
    std::lock_guard<std::recursive_mutex> transition(agent->firmware_transition_mutex);
    return {
        agent->firmware_generation,
        ++agent->firmware_observation_sequence,
    };
}

bool observe_firmware_printers(
    Agent* agent,
    const std::string& body,
    const FirmwareObservationTicket& observation
) {
    if (!agent || !agent->firmware_session) return false;
    std::lock_guard<std::recursive_mutex> transition(agent->firmware_transition_mutex);
    return pandar_plugin_firmware_observe_printers(
        agent->firmware_session,
        reinterpret_cast<const uint8_t*>(body.data()),
        body.size(),
        observation.generation,
        observation.sequence
    ) == 0;
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
                auto callback = message_callback_for(agent, tunnel);
                if (callback) {
                    const auto callback_deadline = std::chrono::steady_clock::time_point(
                        std::chrono::duration_cast<std::chrono::steady_clock::duration>(
                            std::chrono::nanoseconds(
                                static_cast<std::chrono::nanoseconds::rep>(result.origin_tick)
                            )
                        )
                    ) + std::chrono::seconds(2);
                    std::unique_lock<std::timed_mutex> callback_lock(
                        agent->callback_mutex,
                        std::defer_lock
                    );
                    if (!callback_lock.try_lock_until(callback_deadline)) continue;
                    std::lock_guard<std::recursive_mutex> transition(agent->firmware_transition_mutex);
                    if (std::chrono::steady_clock::now() < callback_deadline &&
                        !agent->firmware_thread_stop.load() &&
                        agent->firmware_generation == callback_generation) {
                        callback(dev_id, message);
                    }
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

PluginHttpResult rust_exchange_ticket(const Agent* agent, const std::string& ticket) {
    return pandar_plugin_exchange_ticket(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(ticket.data()),
        ticket.size()
    );
}

PluginHttpResult rust_create_no_auth_session(const Agent* agent) {
    return pandar_plugin_create_no_auth_session(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size()
    );
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
    auto result = pandar_plugin_local_webserver_config();
    std::string body = body_from_result(result);
    if (result.status != 0) return;
    if (const auto hub_url = field_from_json(body, "hub_url"); !hub_url.empty()) {
        if (hub_url != agent->hub_url) {
            clear_persisted_login(agent);
            clear_login_state(agent, false);
        }
        agent->hub_url = hub_url;
        sync_printer_refresh_session(agent);
    }
}

PluginHttpResult rust_get_printers(const Agent* agent) {
    return pandar_plugin_get_printers(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size()
    );
}

PluginHttpResult rust_submit_print(const Agent* agent, const BBL::PrintParams& params) {
    const std::string& display_name = params.task_name.empty() ? params.project_name : params.task_name;
    const std::string& artifact_path = params.filename;
    const auto printer_id = pandar_printer_id_for(agent, params.dev_id);
    return pandar_plugin_submit_print(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size(),
        reinterpret_cast<const uint8_t*>(printer_id.data()),
        printer_id.size(),
        reinterpret_cast<const uint8_t*>(display_name.data()),
        display_name.size(),
        reinterpret_cast<const uint8_t*>(artifact_path.data()),
        artifact_path.size(),
        params.plate_index,
        params.task_use_ams,
        params.task_bed_leveling,
        params.auto_bed_leveling,
        params.task_flow_cali,
        params.auto_flow_cali,
        params.auto_offset_cali,
        params.task_record_timelapse,
        reinterpret_cast<const uint8_t*>(params.ams_mapping.data()),
        params.ams_mapping.size(),
        reinterpret_cast<const uint8_t*>(params.ams_mapping2.data()),
        params.ams_mapping2.size(),
        reinterpret_cast<const uint8_t*>(params.ams_mapping_info.data()),
        params.ams_mapping_info.size()
    );
}

PluginHttpResult rust_operation_json_from_gcode(const std::string& message) {
    return pandar_plugin_operation_json_from_gcode(
        reinterpret_cast<const uint8_t*>(message.data()),
        message.size()
    );
}

PluginHttpResult rust_submit_printer_operation(const Agent* agent, const std::string& printer_id, const std::string& operation_json) {
    return pandar_plugin_submit_printer_operation(
        reinterpret_cast<const uint8_t*>(agent->hub_url.data()),
        agent->hub_url.size(),
        reinterpret_cast<const uint8_t*>(agent->token.data()),
        agent->token.size(),
        reinterpret_cast<const uint8_t*>(printer_id.data()),
        printer_id.size(),
        reinterpret_cast<const uint8_t*>(operation_json.data()),
        operation_json.size()
    );
}

int submit_printer_operation_json(Agent* agent, std::string dev_id, const std::string& operation_json) {
    refresh_local_webserver_config(agent);
    dev_id = pandar_printer_id_for(agent, dev_id);
    if (agent->token.empty() || dev_id.empty()) {
        agent->last_error = R"({"error":"invalid_printer_operation"})";
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }

    auto result = rust_submit_printer_operation(agent, dev_id, operation_json);
    std::string body = body_from_result(result);
    if (result.status != 0) {
        agent->last_error = body;
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }

    agent->last_error.clear();
    return BBL::BAMBU_NETWORK_SUCCESS;
}

struct FirmwareSendAttempt {
    bool handled;
    int result;
    std::uint64_t callback_token;
};

FirmwareSendAttempt begin_firmware_send(
    Agent* agent,
    const std::string& dev_id,
    const std::string& message,
    MessageTunnel tunnel
) {
    const auto normalized_dev_id = studio_dev_id(dev_id);
    const auto printer_id = pandar_printer_id_for(agent, normalized_dev_id);
    if (normalized_dev_id.empty() || printer_id.empty()) {
        return {false, BBL::BAMBU_NETWORK_SUCCESS, 0};
    }
    std::uint64_t callback_token = 0;
    auto result = pandar_plugin_firmware_send(
        agent->firmware_session,
        reinterpret_cast<const uint8_t*>(normalized_dev_id.data()),
        normalized_dev_id.size(),
        reinterpret_cast<const uint8_t*>(printer_id.data()),
        printer_id.size(),
        reinterpret_cast<const uint8_t*>(message.data()),
        message.size(),
        tunnel == MessageTunnel::Cloud ? 0 : 1,
        &callback_token
    );
    auto body = body_from_result(result);
    if (result.status == 2) return {false, BBL::BAMBU_NETWORK_SUCCESS, 0};
    if (result.status != 0) {
        agent->last_error = std::move(body);
        return {true, BBL::BAMBU_NETWORK_ERR_INVALID_RESULT, 0};
    }
    agent->last_error.clear();
    return {true, BBL::BAMBU_NETWORK_SUCCESS, callback_token};
}

int finish_firmware_send(Agent* agent, FirmwareSendAttempt attempt) {
    if (attempt.callback_token != 0) {
        const auto tick = std::chrono::duration_cast<std::chrono::nanoseconds>(
            std::chrono::steady_clock::now().time_since_epoch()
        ).count();
        pandar_plugin_firmware_return_handoff(
            agent->firmware_session,
            attempt.callback_token,
            static_cast<std::uint64_t>(tick)
        );
    }
    return attempt.result;
}

void apply_login_response_body(Agent* agent, const std::string& body) {
    agent->token = field_from_json(body, "token");
    agent->profile_json = object_from_json(body, "profile");
    apply_profile_json(agent, agent->profile_json);
}

void try_no_auth_session(Agent* agent) {
    if (!agent || !agent->token.empty()) return;
    refresh_local_webserver_config(agent);
    auto result = rust_create_no_auth_session(agent);
    std::string body = body_from_result(result);
    if (result.status != 0) return;
    apply_login_response_body(agent, body);
    persist_login_state(agent);
}

bool result_needs_token_refresh(const PluginHttpResult& result) {
    return result.status != 0 && (result.http_code == 401 || result.http_code == 410);
}

bool refresh_no_auth_session(Agent* agent) {
    if (!agent) return false;
    clear_login_state(agent);
    try_no_auth_session(agent);
    return !agent->token.empty();
}

PluginHttpResult get_printers_with_token_refresh(
    Agent* agent,
    std::uint64_t& request_epoch,
    FirmwareObservationTicket& observation
) {
    request_epoch = agent->printer_status_epoch.load();
    observation = begin_firmware_observation(agent);
    auto result = rust_get_printers(agent);
    if (result_needs_token_refresh(result)) {
        body_from_result(result);
        if (refresh_no_auth_session(agent)) {
            request_epoch = agent->printer_status_epoch.load();
            observation = begin_firmware_observation(agent);
            result = rust_get_printers(agent);
        }
    }
    return result;
}


} // namespace pandar::network_plugin
