#pragma once

#include "shim_firmware.hpp"

using namespace pandar::network_plugin;

PANDAR_ABI void bambu_network_enable_multi_machine(void*, bool) {}

PANDAR_ABI int bambu_network_send_message(void* agent, std::string dev_id, std::string message, int, int) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    trace_plugin_event(a, "send_message", dev_id);
    if (dev_id.empty()) dev_id = ensure_selected_machine(a);
    if (dev_id.empty()) return BBL::BAMBU_NETWORK_SUCCESS;
    auto firmware = begin_firmware_send(a, dev_id, message, MessageTunnel::Cloud);
    if (firmware.handled) return finish_firmware_send(a, firmware);
    if (handle_status_request(a, dev_id, message, MessageTunnel::Cloud)) {
        return BBL::BAMBU_NETWORK_SUCCESS;
    }
    auto parsed = rust_operation_json_from_gcode(message);
    std::string operation_json = body_from_result(parsed);
    if (parsed.status == kParseOperation) {
        return submit_printer_operation_json(a, dev_id, operation_json);
    }
    if (parsed.status == kParseInvalidNative) {
        a->last_error = operation_json;
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_connect_printer(void* agent, std::string dev_id, std::string, std::string, std::string, bool) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (dev_id.empty()) return BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED;
    {
        std::lock_guard<std::mutex> lock(a->status_mutex);
        a->active_local_device = studio_dev_id(dev_id);
    }
    emit_local_connect(a, studio_dev_id(dev_id));
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_disconnect_printer(void* agent) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::lock_guard<std::mutex> lock(a->status_mutex);
    a->active_local_device.clear();
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_send_message_to_printer(void* agent, std::string dev_id, std::string message, int, int) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    trace_plugin_event(a, "send_message_to_printer", dev_id);

    auto firmware = begin_firmware_send(a, dev_id, message, MessageTunnel::Local);
    if (firmware.handled) return finish_firmware_send(a, firmware);

    if (handle_status_request(a, dev_id, message, MessageTunnel::Local)) {
        return BBL::BAMBU_NETWORK_SUCCESS;
    }

    auto parsed = rust_operation_json_from_gcode(message);
    std::string operation_json = body_from_result(parsed);
    if (parsed.status != kParseOperation) {
        a->last_error = operation_json;
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    return submit_printer_operation_json(a, dev_id, operation_json);
}

PANDAR_ABI int bambu_network_update_cert(void* agent) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI void bambu_network_install_device_cert(void*, std::string, bool) {}

PANDAR_ABI bool bambu_network_start_discovery(void*, bool, bool) {
    return false;
}

PANDAR_ABI int bambu_network_change_user(void* agent, std::string user_info) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (user_info.empty() || user_info == "{}") {
        clear_persisted_login(a);
        clear_login_state(a);
        return BBL::BAMBU_NETWORK_SUCCESS;
    }
    apply_profile_json(a, user_info);
    persist_login_state(a);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI bool bambu_network_is_user_login(void* agent) {
    auto* a = as_agent(agent);
    if (a) refresh_local_webserver_config(a);
    return a && !a->token.empty();
}

PANDAR_ABI int bambu_network_user_logout(void* agent, bool) {
    auto* a = as_agent(agent);
    if (a) {
        clear_persisted_login(a);
        clear_login_state(a);
    }
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_my_profile(void* agent, std::string token, unsigned int* http_code, std::string* http_body) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (!token.empty()) {
        a->token = std::move(token);
        sync_printer_refresh_session(a);
    }
    if (a->profile_json.empty()) {
        if (http_code) *http_code = 401;
        if (http_body) *http_body = R"({"error":"profile_unavailable"})";
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    success_body(http_code, http_body, studio_profile_body(a));
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_my_token(void* agent, std::string ticket, unsigned int* http_code, std::string* http_body) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    refresh_local_webserver_config(a);
    if (ticket.empty()) {
        if (http_code) *http_code = 401;
        if (http_body) *http_body = R"({"error":"invalid_plugin_ticket"})";
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    auto result = rust_exchange_ticket(a, ticket);
    std::string body;
    if (result.body_ptr && result.body_len > 0) {
        body.assign(reinterpret_cast<char*>(result.body_ptr), result.body_len);
        pandar_plugin_free_with_capacity(result.body_ptr, result.body_len, result.body_cap);
    }
    if (http_code) *http_code = result.http_code;
    if (http_body) *http_body = body;
    if (result.status != 0) {
        a->last_error = body;
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    apply_login_response_body(a, body);
    persist_login_state(a);
    a->last_error.clear();
    success_body(http_code, http_body, studio_token_body(a));
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_user_info(void* agent, int* identifier) {
    if (identifier) *identifier = as_agent(agent) ? 1 : 0;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_user_selected_machine(void* agent, std::string dev_id) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    trace_plugin_event(a, std::string("set_user_selected_machine dev_id=") + dev_id);
    std::lock_guard<std::mutex> lock(a->status_mutex);
    a->selected_machine = std::move(dev_id);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_ping_bind(void* agent, std::string) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_ERR_INVALID_RESULT : BBL::BAMBU_NETWORK_ERR_BIND_FAILED;
}

PANDAR_ABI int bambu_network_bind_detect(void* agent, std::string, std::string, BBL::detectResult& detect) {
    detect = BBL::detectResult{};
    return as_agent(agent) ? BBL::BAMBU_NETWORK_ERR_INVALID_RESULT : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_bind(void* agent, std::string, std::string, std::string, std::string, bool, BBL::OnUpdateStatusFn) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_ERR_BIND_FAILED : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_unbind(void* agent, std::string) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_ERR_UNBIND_FAILED : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_request_bind_ticket(void* agent, std::string* ticket) {
    if (ticket) ticket->clear();
    return as_agent(agent) ? BBL::BAMBU_NETWORK_ERR_INVALID_RESULT : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_query_bind_status(void* agent, std::vector<std::string>, unsigned int* http_code, std::string* http_body) {
    if (http_code) *http_code = 0;
    if (http_body) http_body->clear();
    return as_agent(agent) ? BBL::BAMBU_NETWORK_ERR_QUERY_BIND_INFO_FAILED : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_modify_printer_name(void* agent, std::string, std::string) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_ERR_MODIFY_PRINTER_NAME_FAILED : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_report_consent(void*, std::string) {
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_start_print(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn, BBL::OnWaitFn) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    trace_plugin_event(a, "start_print", params.dev_id);
    refresh_local_webserver_config(a);
    if (cancel_fn && cancel_fn()) return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    if (a->token.empty() || params.dev_id.empty() || params.filename.empty()) {
        if (update_fn) update_fn(BBL::PrintingStageERROR, BBL::BAMBU_NETWORK_ERR_INVALID_RESULT, "Pandar plugin print submission is missing token, printer, or artifact");
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    auto result = rust_submit_print(a, params);
    std::string body = body_from_result(result);
    trace_plugin_event(a, std::string("start_print result=") + std::to_string(result.status) + " http=" + std::to_string(result.http_code), params.dev_id);
    if (result.status != 0) {
        a->last_error = body;
        if (update_fn) update_fn(BBL::PrintingStageERROR, BBL::BAMBU_NETWORK_ERR_INVALID_RESULT, body);
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    a->last_error.clear();
    if (update_fn) update_fn(BBL::PrintingStageFinished, BBL::BAMBU_NETWORK_SUCCESS, "3");
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_start_local_print_with_record(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn, BBL::OnWaitFn wait_fn) {
    if (auto* a = as_agent(agent)) trace_plugin_event(a, "start_local_print_with_record", params.dev_id);
    return bambu_network_start_print(agent, std::move(params), std::move(update_fn), std::move(cancel_fn), std::move(wait_fn));
}

PANDAR_ABI int bambu_network_start_send_gcode_to_sdcard(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn, BBL::OnWaitFn) {
    if (!as_agent(agent)) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (update_fn) update_fn(BBL::PrintingStageERROR, BBL::BAMBU_NETWORK_ERR_INVALID_RESULT, R"({"error":"unsupported_file_transfer"})");
    if (cancel_fn && cancel_fn()) return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    (void)params;
    return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI int bambu_network_start_local_print(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn) {
    if (auto* a = as_agent(agent)) trace_plugin_event(a, "start_local_print", params.dev_id);
    return bambu_network_start_send_gcode_to_sdcard(agent, std::move(params), std::move(update_fn), std::move(cancel_fn), {});
}

PANDAR_ABI int bambu_network_start_sdcard_print(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn) {
    return bambu_network_start_send_gcode_to_sdcard(agent, std::move(params), std::move(update_fn), std::move(cancel_fn), {});
}

PANDAR_ABI int bambu_network_get_user_presets(void*, std::map<std::string, std::map<std::string, std::string>>* user_presets) {
    if (user_presets) user_presets->clear();
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_put_setting(void*, std::string, std::string, std::map<std::string, std::string>*, unsigned int* http_code) {
    if (http_code) *http_code = 0;
    return BBL::BAMBU_NETWORK_ERR_PUT_SETTING_FAILED;
}

PANDAR_ABI int bambu_network_get_setting_list(void*, std::string, BBL::ProgressFn pro_fn, BBL::WasCancelledFn) {
    if (pro_fn) pro_fn(100);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_setting_list2(void*, std::string, BBL::CheckFn, BBL::ProgressFn pro_fn, BBL::WasCancelledFn) {
    if (pro_fn) pro_fn(100);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_delete_setting(void*, std::string) {
    return BBL::BAMBU_NETWORK_ERR_DEL_SETTING_FAILED;
}

PANDAR_ABI int bambu_network_set_extra_http_header(void* agent, std::map<std::string, std::string>) {
    return as_agent(agent) ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
}

PANDAR_ABI int bambu_network_get_my_message(void*, int, int, int, unsigned int* http_code, std::string* http_body) {
    success_body(http_code, http_body, "{}");
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_check_user_task_report(void*, int* task_id, bool* printable) {
    if (task_id) *task_id = 0;
    if (printable) *printable = false;
    return BBL::BAMBU_NETWORK_SUCCESS;
}
