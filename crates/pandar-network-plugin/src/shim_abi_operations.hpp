#pragma once

#include "shim_firmware.hpp"
#include "shim_abi_account.hpp"

using namespace pandar::network_plugin;

namespace pandar::network_plugin {

int dispatch_studio_message(
    Agent* agent,
    const std::string& dev_id,
    const std::string& message,
    MessageTunnel tunnel,
    std::uint64_t local_generation
) {
    auto classified = pandar_plugin_dispatch_studio_message(
        reinterpret_cast<const uint8_t*>(message.data()), message.size()
    );
    const auto kind = classified.kind;
    const auto outcome = classified.outcome;
    const auto abi_status = classified.abi_status;
    auto body = body_from_studio_message(classified);
    if (outcome != 0) {
        return abi_status;
    }
    if (kind == kStudioMessageFirmware) {
        auto firmware = begin_firmware_send(
            agent, dev_id, message, tunnel, local_generation
        );
        return firmware.handled
            ? finish_firmware_send(agent, firmware)
            : BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    if (kind == kStudioMessageGetVersion || kind == kStudioMessagePushAll) {
        auto delivery = pandar_plugin_studio_status_delivery_result(handle_studio_status(
            agent, kind, dev_id, body, tunnel, local_generation
        ));
        body_from_result(delivery);
        return delivery.status;
    }
    if (kind == kStudioMessageOperation) {
        return submit_printer_operation_json(agent, dev_id, body);
    }
    return abi_status;
}

} // namespace pandar::network_plugin

PANDAR_ABI void bambu_network_enable_multi_machine(void* agent, bool) {
    studio_disposition(as_agent(agent), StudioDisposition::EnableMultiMachine);
}

PANDAR_ABI int bambu_network_send_message(void* agent, std::string dev_id, std::string message, int, int) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    trace_plugin_event(a, "send_message", dev_id);
    if (dev_id.empty()) return BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED;
    return dispatch_studio_message(a, dev_id, message, MessageTunnel::Cloud, 0);
}

PANDAR_ABI int bambu_network_connect_printer(void* agent, std::string dev_id, std::string, std::string, std::string, bool) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (dev_id.empty()) return BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED;
    return emit_local_connect(a, studio_dev_id(dev_id))
        ? BBL::BAMBU_NETWORK_SUCCESS
        : BBL::BAMBU_NETWORK_ERR_CONNECT_FAILED;
}

PANDAR_ABI int bambu_network_disconnect_printer(void* agent) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    return pandar_plugin_studio_disconnect_local(a->printer_refresh_session) == 0
        ? BBL::BAMBU_NETWORK_SUCCESS
        : BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI int bambu_network_send_message_to_printer(void* agent, std::string dev_id, std::string message, int, int) {
    auto* a = as_agent(agent);
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
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    trace_plugin_event(a, std::string("set_user_selected_machine dev_id=") + dev_id);
    return pandar_plugin_studio_set_selected(
        a->printer_refresh_session,
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

PANDAR_ABI int bambu_network_bind(void* agent, std::string, std::string, std::string, std::string, std::string, bool, BBL::OnUpdateStatusFn) {
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
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    refresh_local_webserver_config(a);
    return start_studio_print(a, params, update_fn, cancel_fn, wait_fn);
}

PANDAR_ABI int bambu_network_start_local_print_with_record(void* agent, BBL::PrintParams params, BBL::OnUpdateStatusFn update_fn, BBL::WasCancelledFn cancel_fn, BBL::OnWaitFn wait_fn) {
    auto* a = as_agent(agent);
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
    auto* a = as_agent(agent);
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
    if (auto* a = as_agent(agent)) trace_plugin_event(a, "start_local_print", params.dev_id);
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
    return studio_disposition(as_agent(agent), StudioDisposition::UserPresets);
}

PANDAR_ABI int bambu_network_put_setting(void* agent, std::string, std::string, std::map<std::string, std::string>*, unsigned int* http_code) {
    return studio_disposition(
        as_agent(agent), StudioDisposition::PutSetting, nullptr, http_code
    );
}

PANDAR_ABI int bambu_network_get_setting_list(void* agent, std::string, BBL::ProgressFn, BBL::WasCancelledFn) {
    return studio_disposition(as_agent(agent), StudioDisposition::GetSettingList);
}

PANDAR_ABI int bambu_network_get_setting_list2(void* agent, std::string, BBL::CheckFn, BBL::ProgressFn, BBL::WasCancelledFn) {
    return studio_disposition(as_agent(agent), StudioDisposition::GetSettingList2);
}

PANDAR_ABI int bambu_network_delete_setting(void* agent, std::string) {
    return studio_disposition(as_agent(agent), StudioDisposition::DeleteSetting);
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
