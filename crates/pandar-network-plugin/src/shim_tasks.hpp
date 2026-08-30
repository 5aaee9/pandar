#pragma once

#include "shim_print.hpp"
#include "shim_no_auth.hpp"

using namespace pandar::network_plugin;

PANDAR_ABI int bambu_network_get_user_tasks(
    void* agent,
    BBL::TaskQueryParams query,
    std::string* http_body
) {
    auto current = as_agent(agent);
    if (!current) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    refresh_local_webserver_config(current);
    const auto snapshot = printer_request_snapshot(current, {});
    StudioAccountSnapshotContext account_context{current, {}};
    const auto account = studio_account(snapshot, account_context);
    const PluginStudioTaskQuery request{
        plugin_bytes(query.dev_id),
        query.status,
        query.offset,
        query.limit,
    };
    auto result = pandar_plugin_studio_get_tasks_with_session(
        current->connection_session(),
        &account,
        snapshot.account_config_epoch,
        snapshot.session_kind,
        current,
        with_current_account,
        &request
    );
    auto body = body_from_result(result);
    if (http_body) *http_body = body;
    if (result.status != 0) {
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_task_plate_index(
    void* agent,
    std::string task_id,
    int* plate_index
) {
    if (plate_index) *plate_index = -1;
    auto current = as_agent(agent);
    if (!current) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    refresh_local_webserver_config(current);
    const auto snapshot = printer_request_snapshot(current, {});
    StudioAccountSnapshotContext account_context{current, {}};
    const auto account = studio_account(snapshot, account_context);
    auto result = pandar_plugin_studio_get_plate_with_session(
        current->connection_session(),
        &account,
        snapshot.account_config_epoch,
        snapshot.session_kind,
        current,
        with_current_account,
        plugin_bytes(task_id)
    );
    body_from_result(result.http);
    if (result.http.status != 0 || result.plate_index < 0) {
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    if (plate_index) *plate_index = result.plate_index;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_subtask_info(
    void* agent,
    std::string task_id,
    std::string* task_json,
    unsigned int* http_code,
    std::string* http_body
) {
    if (task_json) task_json->clear();
    auto current = as_agent(agent);
    if (!current) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    refresh_local_webserver_config(current);
    const auto snapshot = printer_request_snapshot(current, {});
    StudioAccountSnapshotContext account_context{current, {}};
    const auto account = studio_account(snapshot, account_context);
    auto result = pandar_plugin_studio_get_subtask_with_session(
        current->connection_session(),
        &account,
        snapshot.account_config_epoch,
        snapshot.session_kind,
        current,
        with_current_account,
        plugin_bytes(task_id)
    );
    auto body = body_from_result(result);
    if (http_code) *http_code = result.http_code;
    if (http_body) *http_body = body;
    if (result.status != 0) {
        return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
    }
    if (task_json) *task_json = body;
    return BBL::BAMBU_NETWORK_SUCCESS;
}

PANDAR_ABI int bambu_network_get_slice_info(
    void* agent,
    std::string,
    std::string,
    int,
    std::string* slice_json
) {
    if (slice_json) slice_json->clear();
    auto current = as_agent(agent);
    if (!current) return BBL::BAMBU_NETWORK_ERR_INVALID_HANDLE;
    auto result = pandar_plugin_studio_slice_unavailable();
    body_from_result(result);
    return BBL::BAMBU_NETWORK_ERR_INVALID_RESULT;
}
