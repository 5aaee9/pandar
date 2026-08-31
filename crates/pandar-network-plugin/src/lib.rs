use std::{ffi::c_void, path::PathBuf, slice};

mod account;
mod camera;
mod cancellation;
mod connection;
mod dispatch;
pub mod file_transfer;
pub mod firmware;
mod gcode;
mod h2c;
mod http;
pub mod installer;
mod local_webserver;
mod personal_presets;
mod plugin_core;
mod plugin_session;
mod runtime;
mod studio_abi;
mod studio_disposition;
mod studio_message;
mod studio_policy;
mod studio_print;
pub mod studio_status;

pub use camera::pandar_plugin_camera_url;
pub use connection::{
    ConnectionDeviceVisitor, ConnectionPrinterVisitor, PluginConnectionResult,
    PluginPrinterRefreshLifecycleResult, PluginStudioDeliveryResult, PluginStudioHeartbeatPlan,
    PluginStudioRequestState, ShimCallbackBridge, StudioHeartbeatVisitor, StudioPayloadVisitor,
    StudioRequestVisitor, StudioWorkVisitor, pandar_plugin_connection_claim_delivery,
    pandar_plugin_connection_is_connected, pandar_plugin_connection_printer_eligible,
    pandar_plugin_connection_refresh, pandar_plugin_connection_set_account_epoch,
    pandar_plugin_connection_take_offline, pandar_plugin_connection_take_stream_error,
    pandar_plugin_connection_take_transition, pandar_plugin_connection_visit_printers,
    pandar_plugin_core_printer_refresh, pandar_plugin_printer_refresh_session_create,
    pandar_plugin_printer_refresh_session_destroy,
    pandar_plugin_printer_refresh_session_set_tenant, pandar_plugin_printer_refresh_session_update,
    pandar_plugin_shim_dispatch_connection_transition,
    pandar_plugin_shim_dispatch_offline_deliveries, pandar_plugin_studio_add_subscription,
    pandar_plugin_studio_begin_account_transition, pandar_plugin_studio_claim_delivery,
    pandar_plugin_studio_complete_delivery, pandar_plugin_studio_connect_local,
    pandar_plugin_studio_del_subscription, pandar_plugin_studio_disconnect_local,
    pandar_plugin_studio_finish_account_transition, pandar_plugin_studio_heartbeat_plan,
    pandar_plugin_studio_local_generation, pandar_plugin_studio_prepare_connected,
    pandar_plugin_studio_prepare_message, pandar_plugin_studio_request_snapshot,
    pandar_plugin_studio_selected, pandar_plugin_studio_set_listener,
    pandar_plugin_studio_set_selected, pandar_plugin_studio_status_target_available,
    pandar_plugin_studio_take_work,
};
pub use h2c::pandar_plugin_submit_h2c_auto_nozzle_mapping;
pub use local_webserver::ffi::{
    pandar_plugin_local_webserver_base_url, pandar_plugin_local_webserver_config,
    pandar_plugin_start_local_webserver,
};
pub use personal_presets::{
    PresetBytes, PresetCallbacks, PresetEntry, PresetResult, pandar_plugin_personal_preset_drain,
    pandar_plugin_personal_preset_list, pandar_plugin_personal_preset_mutate,
    pandar_plugin_personal_preset_reset,
};
pub use plugin_core::{
    pandar_plugin_core_account_apply, pandar_plugin_core_account_apply_lifecycle_result,
    pandar_plugin_core_account_drain, pandar_plugin_core_account_identity,
    pandar_plugin_core_connection_session, pandar_plugin_core_create, pandar_plugin_core_destroy,
    pandar_plugin_core_firmware_session,
};
pub use plugin_session::{
    pandar_plugin_create_no_auth_session, pandar_plugin_delete_session,
    pandar_plugin_exchange_ticket, pandar_plugin_no_auth_retryable_connect_failure,
};
pub use studio_abi::{
    NETWORK_AGENT_VERSION, STUDIO_ABI_SERIES, pandar_plugin_local_connect_json,
    pandar_plugin_network_agent_version, pandar_plugin_sync_ams_filaments,
};
pub use studio_message::{
    PluginStudioMessageResult, pandar_plugin_classify_status_request,
    pandar_plugin_dispatch_studio_message, pandar_plugin_operation_json_from_gcode,
};
pub use studio_print::{
    PluginBytes, PluginStudioAccount, PluginStudioCallbacks, PluginStudioModelTask,
    PluginStudioPlateResult, PluginStudioPrintParams, PluginStudioSnapshot, PluginStudioTaskQuery,
    StudioModelTaskVisitor, pandar_plugin_studio_get_model_task,
    pandar_plugin_studio_get_model_task_with_session, pandar_plugin_studio_get_plate,
    pandar_plugin_studio_get_plate_with_session, pandar_plugin_studio_get_subtask,
    pandar_plugin_studio_get_subtask_with_session, pandar_plugin_studio_get_tasks,
    pandar_plugin_studio_get_tasks_with_session, pandar_plugin_studio_slice_unavailable,
    pandar_plugin_studio_start_print,
};

use serde::{Serialize, de::DeserializeOwned};

pub(crate) use runtime::runtime;

use http::{
    AmsMapping, AmsMapping2, AmsMappingInfo, PrintSubmissionBody, calibration_mode, get_json,
    plugin_printer_operation_url, post_json, post_multipart_print,
};

pub const PLUGIN_NAME: &str = "pandar-network-plugin";

const NO_AUTH_CONNECT_FAILURE_STATUS: i32 = 2;

#[derive(Clone, Copy)]
enum RequestKind {
    TicketExchange,
    JobLookup,
    PrintSubmission,
    PrinterOperation,
    H2cAutoNozzleMapping,
    PluginSession,
}

#[repr(C)]
pub struct PluginHttpResult {
    pub status: i32,
    pub http_code: u32,
    pub body_ptr: *mut u8,
    pub body_len: usize,
    pub body_cap: usize,
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_get_jobs(
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    token_ptr: *const u8,
    token_len: usize,
) -> PluginHttpResult {
    get_json(
        hub_url_ptr,
        hub_url_len,
        token_ptr,
        token_len,
        "/api/v1/plugin/jobs",
        RequestKind::JobLookup,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_submit_print(
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    token_ptr: *const u8,
    token_len: usize,
    printer_id_ptr: *const u8,
    printer_id_len: usize,
    filename_ptr: *const u8,
    filename_len: usize,
    artifact_path_ptr: *const u8,
    artifact_path_len: usize,
    plate_id: i64,
    use_ams: bool,
    bed_leveling: bool,
    auto_bed_leveling: i32,
    flow_cali: bool,
    auto_flow_cali: i32,
    auto_offset_cali: i32,
    timelapse: bool,
    ams_mapping_ptr: *const u8,
    ams_mapping_len: usize,
    ams_mapping2_ptr: *const u8,
    ams_mapping2_len: usize,
    ams_mapping_info_ptr: *const u8,
    ams_mapping_info_len: usize,
) -> PluginHttpResult {
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len).and_then(normalize_hub_url) else {
        return invalid_input("invalid_hub_url");
    };
    let Some(token) = read_utf8(token_ptr, token_len).filter(|token| !token.trim().is_empty())
    else {
        return invalid_input("invalid_auth_token");
    };
    let Some(printer_id) = read_utf8(printer_id_ptr, printer_id_len) else {
        return invalid_input("invalid_printer_id");
    };
    let Some(filename) = read_utf8(filename_ptr, filename_len) else {
        return invalid_input("bad_request");
    };
    let Some(artifact_path) = read_utf8(artifact_path_ptr, artifact_path_len) else {
        return invalid_input("artifact_missing");
    };
    let Some(auto_bed_leveling) = calibration_mode(auto_bed_leveling) else {
        return invalid_input("bad_request");
    };
    let Some(auto_flow_cali) = calibration_mode(auto_flow_cali) else {
        return invalid_input("bad_request");
    };
    let Some(auto_offset_cali) = calibration_mode(auto_offset_cali) else {
        return invalid_input("bad_request");
    };
    let Ok(ams_mapping) = parse_optional_json::<AmsMapping>(ams_mapping_ptr, ams_mapping_len)
    else {
        return invalid_input("bad_request");
    };
    let Ok(ams_mapping2) = parse_optional_json::<AmsMapping2>(ams_mapping2_ptr, ams_mapping2_len)
    else {
        return invalid_input("bad_request");
    };
    let Ok(ams_mapping_info) =
        parse_optional_json::<AmsMappingInfo>(ams_mapping_info_ptr, ams_mapping_info_len)
    else {
        return invalid_input("bad_request");
    };
    let artifact_path = PathBuf::from(artifact_path);
    let artifact_len = match std::fs::metadata(&artifact_path) {
        Ok(metadata) if metadata.is_file() => metadata.len(),
        Err(_) => return invalid_input("artifact_missing"),
        Ok(_) => return invalid_input("artifact_missing"),
    };
    if artifact_len == 0 {
        return invalid_input("artifact_empty");
    }
    post_multipart_print(
        &format!("{hub_url}/api/v1/plugin/prints"),
        &token,
        PrintSubmissionBody {
            printer_id,
            filename,
            artifact_path,
            artifact_len,
            plate_id,
            use_ams,
            flow_cali,
            timelapse,
            ams_mapping,
            bed_leveling,
            auto_bed_leveling,
            auto_flow_cali,
            auto_offset_cali,
            ams_mapping2,
            ams_mapping_info,
        },
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_submit_printer_operation(
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    token_ptr: *const u8,
    token_len: usize,
    printer_id_ptr: *const u8,
    printer_id_len: usize,
    operation_json_ptr: *const u8,
    operation_json_len: usize,
) -> PluginHttpResult {
    let (Some(hub_url), Some(token), Some(printer_id), Some(operation_json)) = (
        read_utf8(hub_url_ptr, hub_url_len),
        read_utf8(token_ptr, token_len),
        read_utf8(printer_id_ptr, printer_id_len),
        read_utf8(operation_json_ptr, operation_json_len),
    ) else {
        return invalid_input("bad_request");
    };
    submit_printer_operation_upstream(&hub_url, &token, &printer_id, &operation_json)
}

pub(crate) fn submit_printer_operation_upstream(
    hub_url: &str,
    token: &str,
    printer_id: &str,
    operation_json: &str,
) -> PluginHttpResult {
    let Some(hub_url) = normalize_hub_url(hub_url.to_owned()) else {
        return invalid_input("invalid_hub_url");
    };
    if token.trim().is_empty() {
        return invalid_input("invalid_auth_token");
    }
    if printer_id.trim().is_empty() {
        return invalid_input("invalid_printer_id");
    }
    let Some(operation) = gcode::operation_request_from_json(operation_json) else {
        return invalid_input("invalid_printer_operation");
    };
    let Some(url) = plugin_printer_operation_url(&hub_url, printer_id) else {
        return invalid_input("invalid_printer_id");
    };

    post_json(
        url.as_str(),
        Some(token),
        operation,
        RequestKind::PrinterOperation,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_free(ptr: *mut c_void, len: usize) {
    if !ptr.is_null() && len > 0 {
        unsafe {
            drop(Vec::from_raw_parts(ptr.cast::<u8>(), len, len));
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_free_with_capacity(ptr: *mut c_void, len: usize, cap: usize) {
    if !ptr.is_null() && cap > 0 {
        unsafe {
            drop(Vec::from_raw_parts(ptr.cast::<u8>(), len, cap));
        }
    }
}

fn stable_error_body(error: &str) -> String {
    #[derive(Serialize)]
    struct StableError<'a> {
        error: &'a str,
    }

    serde_json::to_string(&StableError { error }).expect("stable error body is serializable")
}

pub(crate) fn normalize_hub_url(value: String) -> Option<String> {
    let value = value.trim().trim_end_matches('/').to_string();
    if value.is_empty() {
        return None;
    }
    let url = reqwest::Url::parse(&value).ok()?;
    let host = url.host_str()?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback());
    if url.scheme() == "https" || url.scheme() == "http" && loopback {
        Some(value)
    } else {
        None
    }
}

fn invalid_input(error: &str) -> PluginHttpResult {
    result(1, 400, stable_error_body(error))
}

fn network_error() -> PluginHttpResult {
    result(1, 0, stable_error_body("hub_unavailable"))
}

fn result(status: i32, http_code: u32, body: impl Into<String>) -> PluginHttpResult {
    let mut body = body.into().into_bytes();
    let body_ptr = body.as_mut_ptr();
    let body_len = body.len();
    let body_cap = body.capacity();
    std::mem::forget(body);
    PluginHttpResult {
        status,
        http_code,
        body_ptr,
        body_len,
        body_cap,
    }
}

fn read_utf8(ptr: *const u8, len: usize) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    std::str::from_utf8(unsafe { slice::from_raw_parts(ptr, len) })
        .ok()
        .map(ToOwned::to_owned)
}

fn parse_optional_json<T: DeserializeOwned>(ptr: *const u8, len: usize) -> Result<Option<T>, ()> {
    let value = read_utf8(ptr, len).ok_or(())?;
    if value.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&value).map(Some).map_err(|_| ())
}

#[cfg(test)]
#[test]
fn hub_url_normalization_allows_loopback_http() {
    assert_eq!(
        normalize_hub_url("http://localhost:3000/".to_owned()),
        Some("http://localhost:3000".to_owned())
    );
    assert_eq!(
        normalize_hub_url("http://127.0.0.1:8080/".to_owned()),
        Some("http://127.0.0.1:8080".to_owned())
    );
}
