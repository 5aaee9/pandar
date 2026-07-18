#pragma once

#include "shim_firmware.hpp"

using namespace pandar::network_plugin;


PANDAR_ABI int bambu_network_get_user_print_info(void* agent, unsigned int* http_code, std::string* http_body) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    refresh_local_webserver_config(a);
    if (a->token.empty()) {
        if (http_code) *http_code = 401;
        if (http_body) *http_body = R"({"error":"invalid_auth_token"})";
        return BBL::BAMBU_NETWORK_ERR_GET_USER_PRINTINFO_FAILED;
    }
    std::uint64_t request_epoch = 0;
    FirmwareObservationTicket observation;
    auto result = get_printers_with_token_refresh(a, request_epoch, observation);
    if (http_code) *http_code = result.http_code;
    auto body = body_from_result(result);
    if (result.status == 0 && !remember_printer_connections(a, body, request_epoch)) {
        trace_plugin_event(a, "get_user_print_info discarded after login change");
        if (http_code) *http_code = 401;
        if (http_body) *http_body = R"({"error":"invalid_auth_token"})";
        return BBL::BAMBU_NETWORK_ERR_GET_USER_PRINTINFO_FAILED;
    }
    if (result.status == 0) observe_firmware_printers(a, body, observation);
    trace_plugin_event(a, std::string("get_user_print_info status=") + std::to_string(result.status));
    if (http_body) *http_body = body;
    if (result.status != 0) return BBL::BAMBU_NETWORK_ERR_GET_USER_PRINTINFO_FAILED;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_user_tasks(void* agent, BBL::TaskQueryParams, std::string* http_body) {
    if (!as_agent(agent)) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    if (http_body) *http_body = R"({"total":0,"hits":[]})";
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_printer_firmware(void* agent, std::string dev_id, unsigned* http_code, std::string* http_body) {
    auto* a = as_agent(agent);
    if (!a) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    refresh_local_webserver_config(a);
    const auto normalized_dev_id = studio_dev_id(dev_id);
    const auto printer_id = pandar_printer_id_for(a, normalized_dev_id);
    auto result = pandar_plugin_firmware_catalog(
        a->firmware_session,
        reinterpret_cast<const uint8_t*>(normalized_dev_id.data()),
        normalized_dev_id.size(),
        reinterpret_cast<const uint8_t*>(printer_id.data()),
        printer_id.size()
    );
    if (http_code) *http_code = result.http_code;
    auto body = body_from_result(result);
    if (http_body) *http_body = body;
    if (result.status != 0) {
        a->last_error = std::move(body);
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    a->last_error.clear();
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_task_plate_index(void*, std::string, int* plate_index) {
    if (plate_index) *plate_index = -1;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_subtask_info(void*, std::string, std::string* task_json, unsigned int* http_code, std::string* http_body) {
    if (task_json) task_json->clear();
    success_body(http_code, http_body, "{}");
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_slice_info(void*, std::string, std::string, int, std::string* slice_json) {
    if (slice_json) slice_json->clear();
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_camera_url(void* agent, std::string dev_id, std::function<void(std::string)> callback) {
    auto* a = as_agent(agent);
    if (callback) callback(camera_url_for(a, dev_id));
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_camera_url_for_golive(void* agent, std::string dev_id, std::string, std::function<void(std::string)> callback) {
    auto* a = as_agent(agent);
    if (callback) callback(camera_url_for(a, dev_id));
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_hms_snapshot(void*, std::string&, std::string&, std::function<void(std::string, int)> callback) {
    if (callback) callback({}, -1);
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_design_staffpick(void*, int, int, std::function<void(std::string)> cb) {
    if (cb) cb(R"({"list":[],"total":0})");
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_start_publish(void*, BBL::PublishParams, BBL::OnUpdateStatusFn, BBL::WasCancelledFn, std::string* out) {
    if (out) out->clear();
    return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI int bambu_network_get_model_publish_url(void*, std::string* url) {
    if (url) *url = "https://makerworld.com/";
    return BBL::BAMBU_NETWORK_SUCCESS;
}

class BBLModelTask;

PANDAR_ABI int bambu_network_get_subtask(void*, BBLModelTask*, std::function<void(BBLModelTask*)>) {
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_model_mall_home_url(void*, std::string* url) {
    if (url) *url = "https://makerworld.com/";
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_model_mall_detail_url(void*, std::string* url, std::string id) {
    if (url) *url = std::string("https://makerworld.com/models/") + id;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_put_model_mall_rating(void*, int, int, std::string, std::vector<std::string>, unsigned int& http_code, std::string& http_error) {
    http_code = 0;
    http_error.clear();
    return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI int bambu_network_get_oss_config(void*, std::string& config, std::string, unsigned int& http_code, std::string& http_error) {
    config.clear();
    http_code = 0;
    http_error.clear();
    return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI int bambu_network_put_rating_picture_oss(void*, std::string&, std::string& pic_oss_path, std::string, int, unsigned int& http_code, std::string& http_error) {
    pic_oss_path.clear();
    http_code = 0;
    http_error.clear();
    return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI int bambu_network_get_model_mall_rating(void*, int, std::string& rating_result, unsigned int& http_code, std::string& http_error) {
    rating_result.clear();
    http_code = 0;
    http_error.clear();
    return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}

PANDAR_ABI int bambu_network_get_mw_user_preference(void*, std::function<void(std::string)> cb) {
    if (cb) cb(R"({"recommendStatus":0})");
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_mw_user_4ulist(void*, int, int, std::function<void(std::string)> cb) {
    if (cb) cb(R"({"list":[],"total":0})");
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_filament_spools(void*, BBL::FilamentQueryParams, std::string* http_body) {
    if (http_body) *http_body = "{}";
    return BBL::BAMBU_NETWORK_ERR_GET_FILAMENTS_FAILED;
}

PANDAR_ABI int bambu_network_create_filament_spool(void*, std::string, std::string* http_body) {
    if (http_body) *http_body = "{}";
    return BBL::BAMBU_NETWORK_ERR_CREATE_FILAMENT_FAILED;
}

PANDAR_ABI int bambu_network_update_filament_spool(void*, std::string, std::string, std::string* http_body) {
    if (http_body) *http_body = "{}";
    return BBL::BAMBU_NETWORK_ERR_UPDATE_FILAMENT_FAILED;
}

PANDAR_ABI int bambu_network_delete_filament_spools(void*, BBL::FilamentDeleteParams, std::string* http_body) {
    if (http_body) *http_body = "{}";
    return BBL::BAMBU_NETWORK_ERR_DELETE_FILAMENT_FAILED;
}

PANDAR_ABI int bambu_network_get_filament_config(void*, std::string* http_body) {
    if (http_body) *http_body = "{}";
    return BBL::BAMBU_NETWORK_ERR_GET_FILAMENT_CONFIG_FAILED;
}

PANDAR_ABI int bambu_network_track_enable(void*, bool) { return BBL::BAMBU_NETWORK_SUCCESS; }
PANDAR_ABI int bambu_network_track_remove_files(void*) { return BBL::BAMBU_NETWORK_SUCCESS; }
PANDAR_ABI int bambu_network_track_event(void*, std::string, std::string) { return BBL::BAMBU_NETWORK_SUCCESS; }
PANDAR_ABI int bambu_network_track_header(void*, std::string) { return BBL::BAMBU_NETWORK_SUCCESS; }
PANDAR_ABI int bambu_network_track_update_property(void*, std::string, std::string, std::string) { return BBL::BAMBU_NETWORK_SUCCESS; }
PANDAR_ABI int bambu_network_track_get_property(void*, std::string, std::string& value, std::string) {
    value.clear();
    return BBL::BAMBU_NETWORK_SUCCESS;
}
