#pragma once

#include "shim_firmware.hpp"

using namespace pandar::network_plugin;


PANDAR_ABI int bambu_network_get_user_print_info(void* agent, unsigned int* http_code, std::string* http_body) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    refresh_local_webserver_config(a);
    std::unique_lock<std::mutex> request(a->printer_refresh_request_mutex);
    PrinterRefreshAdapterState adapter_state{a};
    auto lifecycle = pandar_plugin_printer_refresh_with_session(
        a->printer_refresh_session,
        kPrinterRefreshStudioPrintInfo,
        a,
        with_current_account,
        printer_refresh_adapter(&adapter_state)
    );
    const auto status = lifecycle.http.status;
    const auto result_http_code = lifecycle.http.http_code;
    auto body = body_from_result(lifecycle.http);
    request.unlock();
    dispatch_connection_transition(a, lifecycle.connection);
    dispatch_printer_offline_transitions(a, std::move(adapter_state.offline));
    if (lifecycle.snapshot_current == 0 && status == 0) {
        trace_plugin_event(a, "get_user_print_info discarded after login change");
    }
    trace_plugin_event(a, std::string("get_user_print_info status=") + std::to_string(status));
    if (http_code) *http_code = result_http_code;
    if (http_body) *http_body = std::move(body);
    return status;
}

PANDAR_ABI int bambu_network_get_printer_firmware(void* agent, std::string dev_id, unsigned* http_code, std::string* http_body) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    refresh_local_webserver_config(a);
    const auto normalized_dev_id = studio_dev_id(dev_id);
    const auto snapshot = printer_request_snapshot(a, normalized_dev_id);
    auto admission = pandar_plugin_studio_request_admitted(
        snapshot.printer_authorized, snapshot.account_transition_pending
    );
    if (admission.status != 0) {
        const auto status = admission.status;
        if (http_code) *http_code = admission.http_code;
        auto body = body_from_result(admission);
        if (http_body) *http_body = std::move(body);
        return status;
    }
    body_from_result(admission);
    auto upstream = firmware_catalog_from_snapshot(
        pandar_plugin_firmware_catalog,
        a->firmware_session,
        normalized_dev_id,
        snapshot
    );
    const auto upstream_status = upstream.status;
    const auto upstream_http_code = upstream.http_code;
    auto body = body_from_result(upstream);
    auto result = pandar_plugin_studio_firmware_catalog_result(
        upstream_status,
        upstream_http_code,
        reinterpret_cast<const uint8_t*>(body.data()), body.size(),
        printer_request_snapshot_current(a, snapshot)
    );
    body = body_from_result(result);
    if (http_code) *http_code = result.http_code;
    if (http_body) *http_body = body;
    return result.status;
}

PANDAR_ABI int bambu_network_get_camera_url(void* agent, std::string dev_id, std::function<void(std::string)> callback) {
    auto* a = as_agent(agent);
    auto result = pandar_plugin_camera_access_result(a != nullptr);
    body_from_result(result);
    if (callback) callback({});
    return result.status;
}

PANDAR_ABI int bambu_network_get_camera_url_for_golive(void* agent, std::string dev_id, std::string, std::function<void(std::string)> callback) {
    auto* a = as_agent(agent);
    auto result = pandar_plugin_camera_access_result(a != nullptr);
    body_from_result(result);
    if (callback) callback({});
    return result.status;
}

PANDAR_ABI int bambu_network_get_hms_snapshot(void* agent, std::string& current, std::string& history, std::function<void(std::string, int)>) {
    current.clear();
    history.clear();
    return studio_disposition(as_agent(agent), StudioDisposition::HmsSnapshot);
}

PANDAR_ABI int bambu_network_get_design_staffpick(void* agent, int, int, std::function<void(std::string)>) {
    return studio_disposition(as_agent(agent), StudioDisposition::DesignStaffPick);
}

PANDAR_ABI int bambu_network_start_publish(void* agent, BBL::PublishParams, BBL::OnUpdateStatusFn, BBL::WasCancelledFn, std::string* out) {
    if (out) out->clear();
    return studio_disposition(as_agent(agent), StudioDisposition::StartPublish);
}

PANDAR_ABI int bambu_network_get_model_publish_url(void* agent, std::string* url) {
    if (url) url->clear();
    return studio_disposition(as_agent(agent), StudioDisposition::ModelPublishUrl);
}

PANDAR_ABI int bambu_network_get_subtask(
    void* agent,
    Slic3r::BBLModelTask* task,
    std::function<void(Slic3r::BBLModelTask*)> callback
) {
    auto* current = as_agent(agent);
    if (!current) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    return enqueue_model_task(current, task, std::move(callback))
        ? BBL::BAMBU_NETWORK_SUCCESS
        : BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI int bambu_network_get_model_mall_home_url(void* agent, std::string* url) {
    if (url) url->clear();
    return studio_disposition(as_agent(agent), StudioDisposition::ModelMallHome);
}

PANDAR_ABI int bambu_network_get_model_mall_detail_url(void* agent, std::string* url, std::string) {
    if (url) url->clear();
    return studio_disposition(as_agent(agent), StudioDisposition::ModelMallDetail);
}

PANDAR_ABI int bambu_network_put_model_mall_rating(void* agent, int, int, std::string, std::vector<std::string>, unsigned int& http_code, std::string& http_error) {
    return studio_disposition(
        as_agent(agent), StudioDisposition::PutModelRating, &http_error, &http_code
    );
}

PANDAR_ABI int bambu_network_get_oss_config(void* agent, std::string& config, std::string, unsigned int& http_code, std::string& http_error) {
    config.clear();
    return studio_disposition(
        as_agent(agent), StudioDisposition::OssConfig, &http_error, &http_code
    );
}

PANDAR_ABI int bambu_network_put_rating_picture_oss(void* agent, std::string&, std::string& pic_oss_path, std::string, int, unsigned int& http_code, std::string& http_error) {
    pic_oss_path.clear();
    return studio_disposition(
        as_agent(agent), StudioDisposition::PutRatingPicture, &http_error, &http_code
    );
}

PANDAR_ABI int bambu_network_get_model_mall_rating(void* agent, int, std::string& rating_result, unsigned int& http_code, std::string& http_error) {
    rating_result.clear();
    return studio_disposition(
        as_agent(agent), StudioDisposition::GetModelRating, &http_error, &http_code
    );
}

PANDAR_ABI int bambu_network_get_mw_user_preference(void* agent, std::function<void(std::string)>) {
    return studio_disposition(as_agent(agent), StudioDisposition::MakerWorldPreference);
}

PANDAR_ABI int bambu_network_get_mw_user_4ulist(void* agent, int, int, std::function<void(std::string)>) {
    return studio_disposition(as_agent(agent), StudioDisposition::MakerWorldForYou);
}

PANDAR_ABI int bambu_network_get_filament_spools(void* agent, BBL::FilamentQueryParams, std::string* http_body) {
    std::string body;
    const auto status = studio_disposition(as_agent(agent), StudioDisposition::GetFilaments, &body);
    if (http_body) *http_body = std::move(body);
    return status;
}

PANDAR_ABI int bambu_network_create_filament_spool(void* agent, std::string, std::string* http_body) {
    std::string body;
    const auto status = studio_disposition(as_agent(agent), StudioDisposition::CreateFilament, &body);
    if (http_body) *http_body = std::move(body);
    return status;
}

PANDAR_ABI int bambu_network_update_filament_spool(void* agent, std::string, std::string, std::string* http_body) {
    std::string body;
    const auto status = studio_disposition(as_agent(agent), StudioDisposition::UpdateFilament, &body);
    if (http_body) *http_body = std::move(body);
    return status;
}

PANDAR_ABI int bambu_network_delete_filament_spools(void* agent, BBL::FilamentDeleteParams, std::string* http_body) {
    std::string body;
    const auto status = studio_disposition(as_agent(agent), StudioDisposition::DeleteFilament, &body);
    if (http_body) *http_body = std::move(body);
    return status;
}

PANDAR_ABI int bambu_network_get_filament_config(void* agent, std::string* http_body) {
    std::string body;
    const auto status = studio_disposition(as_agent(agent), StudioDisposition::GetFilamentConfig, &body);
    if (http_body) *http_body = std::move(body);
    return status;
}

PANDAR_ABI int bambu_network_track_enable(void* agent, bool) { return studio_disposition(as_agent(agent), StudioDisposition::TrackEnable); }
PANDAR_ABI int bambu_network_track_remove_files(void* agent) { return studio_disposition(as_agent(agent), StudioDisposition::TrackRemoveFiles); }
PANDAR_ABI int bambu_network_track_event(void* agent, std::string, std::string) { return studio_disposition(as_agent(agent), StudioDisposition::TrackEvent); }
PANDAR_ABI int bambu_network_track_header(void* agent, std::string) { return studio_disposition(as_agent(agent), StudioDisposition::TrackHeader); }
PANDAR_ABI int bambu_network_track_update_property(void* agent, std::string, std::string, std::string) { return studio_disposition(as_agent(agent), StudioDisposition::TrackUpdateProperty); }
PANDAR_ABI int bambu_network_track_get_property(void* agent, std::string, std::string& value, std::string) {
    value.clear();
    return studio_disposition(as_agent(agent), StudioDisposition::TrackGetProperty);
}
