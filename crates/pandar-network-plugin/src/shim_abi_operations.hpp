#pragma once

#include "shim_firmware.hpp"
#include "shim_abi_account.hpp"

using namespace pandar::network_plugin;

PANDAR_ABI void bambu_network_enable_multi_machine(void* agent, bool) {
    studio_disposition(as_agent(agent), StudioDisposition::EnableMultiMachine);
}

PANDAR_ABI int bambu_network_send_message(void* agent, std::string dev_id, std::string message, int, int) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    trace_plugin_event(a, "send_message", dev_id);
    if (dev_id.empty()) return BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED;
    return dispatch_studio_message(a, dev_id, message, MessageTunnel::Cloud, 0);
}

PANDAR_ABI int bambu_network_connect_printer(void* agent, std::string dev_id, std::string, std::string, std::string, bool) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (dev_id.empty()) return BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED;
    return dispatch_connect_printer_local(a, studio_dev_id(dev_id));
}

PANDAR_ABI int bambu_network_disconnect_printer(void* agent) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    return pandar_plugin_studio_disconnect_local(a->connection_session()) == 0
        ? BBL::BAMBU_NETWORK_SUCCESS
        : BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI int bambu_network_send_message_to_printer(void* agent, std::string dev_id, std::string message, int, int) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    trace_plugin_event(a, "send_message_to_printer", dev_id);
    const auto local_generation = current_local_printer_generation(a, studio_dev_id(dev_id));
    if (local_generation == 0) {
        return BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED;
    }

    return dispatch_studio_message(
        a, dev_id, message, MessageTunnel::Local, local_generation
    );
}

PANDAR_ABI int bambu_network_update_cert(void* agent) {
    return studio_disposition(as_agent(agent), StudioDisposition::UpdateCert);
}

PANDAR_ABI void bambu_network_install_device_cert(void* agent, std::string, bool) {
    studio_disposition(as_agent(agent), StudioDisposition::InstallCert);
}

PANDAR_ABI bool bambu_network_start_discovery(void* agent, bool, bool) {
    studio_disposition(as_agent(agent), StudioDisposition::StartDiscovery);
    return false;
}

PANDAR_ABI int bambu_network_get_user_info(void* agent, int* identifier) {
    if (!as_agent(agent)) {
        if (identifier) *identifier = 0;
        return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    }
    if (identifier) *identifier = 1;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_set_user_selected_machine(void* agent, std::string dev_id) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    trace_plugin_event(a, std::string("set_user_selected_machine dev_id=") + dev_id);
    return pandar_plugin_studio_set_selected(
        a->connection_session(),
        reinterpret_cast<const uint8_t*>(dev_id.data()), dev_id.size()
    ) == 0 ? BBL::BAMBU_NETWORK_SUCCESS : BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI int bambu_network_ping_bind(void* agent, std::string) {
    return studio_disposition(as_agent(agent), StudioDisposition::PingBind);
}

PANDAR_ABI int bambu_network_bind_detect(void* agent, std::string, std::string, BBL::detectResult& detect) {
    detect = BBL::detectResult{};
    return studio_disposition(as_agent(agent), StudioDisposition::BindDetect);
}

PANDAR_ABI int bambu_network_bind(
    void* agent, std::string, std::string, std::string, std::string,
    PANDAR_STUDIO_BIND_MODEL_PARAMETER bool, BBL::OnUpdateStatusFn
) {
    return studio_disposition(as_agent(agent), StudioDisposition::Bind);
}

PANDAR_ABI int bambu_network_unbind(void* agent, std::string) {
    return studio_disposition(as_agent(agent), StudioDisposition::Unbind);
}

PANDAR_ABI int bambu_network_request_bind_ticket(void* agent, std::string* ticket) {
    if (ticket) ticket->clear();
    return studio_disposition(as_agent(agent), StudioDisposition::BindTicket);
}

PANDAR_ABI int bambu_network_query_bind_status(void* agent, std::vector<std::string>, unsigned int* http_code, std::string* http_body) {
    std::string body;
    const auto status = studio_disposition(
        as_agent(agent), StudioDisposition::BindStatus, &body, http_code
    );
    if (http_body) *http_body = std::move(body);
    return status;
}

PANDAR_ABI int bambu_network_modify_printer_name(void* agent, std::string, std::string) {
    return studio_disposition(as_agent(agent), StudioDisposition::ModifyPrinterName);
}

PANDAR_ABI int bambu_network_report_consent(void* agent, std::string) {
    return studio_disposition(as_agent(agent), StudioDisposition::Consent);
}

PANDAR_ABI int bambu_network_start_print(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn, BBL::OnWaitFn wait_fn) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    refresh_local_webserver_config(a);
    return start_studio_print(a, params, update_fn, cancel_fn, wait_fn);
}

PANDAR_ABI int bambu_network_start_local_print_with_record(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn, BBL::OnWaitFn wait_fn) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::string body;
    const auto status = studio_disposition(a, StudioDisposition::LocalPrintWithRecord, &body);
    if (update_fn) update_fn(BBL::PrintingStageERROR, status, std::move(body));
    (void)params;
    (void)cancel_fn;
    (void)wait_fn;
    return status;
}

int unsupported_direct_print(
    void* agent,
    BBL::PrintParams params,
    BBL::OnUpdateStatusFn update_fn,
    StudioDisposition operation
) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    std::string body;
    const auto status = studio_disposition(a, operation, &body);
    if (update_fn) update_fn(BBL::PrintingStageERROR, status, std::move(body));
    (void)params;
    return status;
}

PANDAR_ABI int bambu_network_start_send_gcode_to_sdcard(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn, BBL::OnWaitFn) {
    (void)cancel_fn;
    return unsupported_direct_print(
        agent, std::move(params), std::move(update_fn), StudioDisposition::SendGcodeToSdcard
    );
}

PANDAR_ABI int bambu_network_start_local_print(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn) {
    if (auto a = as_agent(agent)) trace_plugin_event(a, "start_local_print", params.dev_id);
    (void)cancel_fn;
    return unsupported_direct_print(
        agent, std::move(params), std::move(update_fn), StudioDisposition::LocalPrint
    );
}

PANDAR_ABI int bambu_network_start_sdcard_print(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn) {
    (void)cancel_fn;
    return unsupported_direct_print(
        agent, std::move(params), std::move(update_fn), StudioDisposition::SdcardPrint
    );
}

PANDAR_ABI int bambu_network_get_user_presets(void* agent, std::map<std::string, std::map<std::string, std::string>>* user_presets) {
    if (user_presets) user_presets->clear();
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    auto account_copy = personal_preset_account(a);
    auto account = account_copy.view();
    if (!user_presets) return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    PresetDrainAdapter adapter{user_presets};
    return pandar_plugin_personal_preset_drain(&account, &adapter, preset_drain);
}

PANDAR_ABI int bambu_network_put_setting(void* agent, std::string id, std::string name, std::map<std::string, std::string>* values, unsigned int* http_code) {
    auto a = as_agent(agent);
    if (!a) {
        if (http_code) *http_code = 0;
        return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    }
    if (!values) {
        if (http_code) *http_code = 400;
        return BBL::BAMBU_NETWORK_ERR_PUT_SETTING_FAILED;
    }
    auto response = preset_mutate(a, 2, id, name, values);
    if (http_code) *http_code = response.http_code;
    if (values && response.updated_time > 0)
        (*values)["updated_time"] = std::to_string(response.updated_time);
    if (values && response.code > 0) (*values)["code"] = std::to_string(response.code);
    take_preset_id(response);
    return response.status;
}

PANDAR_ABI int bambu_network_get_setting_list(void* agent, std::string version, BBL::ProgressFn progress, BBL::WasCancelledFn cancel) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    AgentCallLease lease(a);
    if (!lease) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    auto account_copy = personal_preset_account(a);
    auto account = account_copy.view();
    PresetListAdapter adapter{a, account.account_epoch, account.config_epoch, {}, std::move(progress), std::move(cancel)};
    return pandar_plugin_personal_preset_list(
        &account, preset_bytes(version), {&adapter, nullptr, preset_progress, preset_cancel, preset_current}
    );
}

PANDAR_ABI int bambu_network_get_setting_list2(void* agent, std::string version, BBL::CheckFn check, BBL::ProgressFn progress, BBL::WasCancelledFn cancel) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    AgentCallLease lease(a);
    if (!lease) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    auto account_copy = personal_preset_account(a);
    auto account = account_copy.view();
    PresetListAdapter adapter{a, account.account_epoch, account.config_epoch, std::move(check), std::move(progress), std::move(cancel)};
    return pandar_plugin_personal_preset_list(
        &account, preset_bytes(version), {&adapter, preset_check, preset_progress, preset_cancel, preset_current}
    );
}

PANDAR_ABI int bambu_network_delete_setting(void* agent, std::string id) {
    auto a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    auto response = preset_mutate(a, 3, id, {}, nullptr);
    take_preset_id(response);
    return response.status;
}

PANDAR_ABI int bambu_network_set_extra_http_header(void* agent, std::map<std::string, std::string>) {
    return studio_disposition(as_agent(agent), StudioDisposition::ExtraHttpHeader);
}

PANDAR_ABI int bambu_network_get_my_message(void* agent, int, int, int, unsigned int* http_code, std::string* http_body) {
    std::string body;
    const auto status = studio_disposition(
        as_agent(agent), StudioDisposition::UserMessages, &body, http_code
    );
    if (http_body) *http_body = std::move(body);
    return status;
}

PANDAR_ABI int bambu_network_check_user_task_report(void* agent, int* task_id, bool* printable) {
    if (task_id) *task_id = 0;
    if (printable) *printable = false;
    return studio_disposition(as_agent(agent), StudioDisposition::UserTaskReport);
}
