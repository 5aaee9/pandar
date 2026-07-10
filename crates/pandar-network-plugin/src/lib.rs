use std::{ffi::c_void, path::PathBuf, slice};

mod gcode;
mod http;
pub mod installer;
mod local_webserver;
mod printer_refresh;
mod studio_status;

pub use printer_refresh::{
    pandar_plugin_printer_refresh, pandar_plugin_printer_refresh_session_create,
    pandar_plugin_printer_refresh_session_destroy, pandar_plugin_printer_refresh_session_update,
};

use serde::{Serialize, de::DeserializeOwned};

use gcode::{PrinterOperation, operation_json_from_gcode};
use http::{
    AmsMapping, AmsMapping2, AmsMappingInfo, PrintSubmissionBody, get_json,
    plugin_printer_operation_url, post_json, post_multipart_print,
};

pub const PLUGIN_NAME: &str = "pandar-network-plugin";

#[derive(Clone, Copy)]
enum RequestKind {
    TicketExchange,
    PrinterLookup,
    JobLookup,
    PrintSubmission,
    PrinterOperation,
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
pub extern "C" fn pandar_plugin_start_local_webserver(
    web_url_ptr: *const u8,
    web_url_len: usize,
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    web_configured: bool,
    hub_configured: bool,
) -> PluginHttpResult {
    let Some(web_url) = read_utf8(web_url_ptr, web_url_len) else {
        return invalid_input("invalid_target_server");
    };
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len) else {
        return invalid_input("invalid_target_server");
    };
    local_webserver::start(web_url, hub_url, web_configured, hub_configured)
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_local_webserver_base_url() -> PluginHttpResult {
    local_webserver::base_url()
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_local_webserver_config() -> PluginHttpResult {
    local_webserver::config()
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_exchange_ticket(
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    ticket_ptr: *const u8,
    ticket_len: usize,
) -> PluginHttpResult {
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len).and_then(normalize_hub_url) else {
        return invalid_input("invalid_hub_url");
    };
    let Some(ticket) = read_utf8(ticket_ptr, ticket_len).filter(|ticket| !ticket.trim().is_empty())
    else {
        return invalid_input("invalid_plugin_ticket");
    };
    post_json(
        &format!("{hub_url}/api/v1/plugin/login-tickets/exchange"),
        None,
        TicketExchangeRequest { ticket: &ticket },
        RequestKind::TicketExchange,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_create_no_auth_session(
    hub_url_ptr: *const u8,
    hub_url_len: usize,
) -> PluginHttpResult {
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len).and_then(normalize_hub_url) else {
        return invalid_input("invalid_hub_url");
    };
    post_json(
        &format!("{hub_url}/api/v1/plugin/no-auth-session"),
        None,
        EmptyRequest {},
        RequestKind::TicketExchange,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_get_printers(
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
        "/api/v1/plugin/printers",
        RequestKind::PrinterLookup,
    )
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
    flow_cali: bool,
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
    let artifact_path = PathBuf::from(artifact_path);
    let artifact_len = match std::fs::metadata(&artifact_path) {
        Ok(metadata) if metadata.is_file() => metadata.len(),
        Err(_) => return invalid_input("artifact_missing"),
        Ok(_) => return invalid_input("artifact_missing"),
    };
    if artifact_len == 0 {
        return invalid_input("artifact_empty");
    }
    let ams_mapping = parse_optional_json::<AmsMapping>(ams_mapping_ptr, ams_mapping_len);
    let ams_mapping2 = parse_optional_json::<AmsMapping2>(ams_mapping2_ptr, ams_mapping2_len);
    let ams_mapping_info =
        parse_optional_json::<AmsMappingInfo>(ams_mapping_info_ptr, ams_mapping_info_len);

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
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len).and_then(normalize_hub_url) else {
        return invalid_input("invalid_hub_url");
    };
    let Some(token) = read_utf8(token_ptr, token_len).filter(|token| !token.trim().is_empty())
    else {
        return invalid_input("invalid_auth_token");
    };
    let Some(printer_id) = read_utf8(printer_id_ptr, printer_id_len)
        .filter(|printer_id| !printer_id.trim().is_empty())
    else {
        return invalid_input("invalid_printer_id");
    };
    let Some(operation) = read_utf8(operation_json_ptr, operation_json_len)
        .and_then(|body| PrinterOperation::from_json(&body))
    else {
        return invalid_input("invalid_printer_operation");
    };
    let Some(url) = plugin_printer_operation_url(&hub_url, &printer_id) else {
        return invalid_input("invalid_printer_id");
    };

    post_json(
        url.as_str(),
        Some(&token),
        operation,
        RequestKind::PrinterOperation,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_operation_json_from_gcode(
    message_ptr: *const u8,
    message_len: usize,
) -> PluginHttpResult {
    let Some(message) = read_utf8(message_ptr, message_len) else {
        return invalid_input("unsupported_printer_operation");
    };
    match operation_json_from_gcode(&message) {
        Some(operation) => result(
            0,
            200,
            serde_json::to_string(&operation).expect("printer operation is serializable"),
        ),
        None => invalid_input("unsupported_printer_operation"),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_printer_telemetry_json(
    printer_ptr: *const u8,
    printer_len: usize,
) -> PluginHttpResult {
    let printer_json = read_utf8(printer_ptr, printer_len).unwrap_or_default();
    result(
        0,
        200,
        studio_status::printer_telemetry_fragment(&printer_json),
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
    format!(r#"{{"error":"{error}"}}"#)
}

fn normalize_hub_url(value: String) -> Option<String> {
    let value = value.trim().trim_end_matches('/').to_string();
    if value.is_empty() {
        return None;
    }
    let url = reqwest::Url::parse(&value).ok()?;
    if matches!(url.scheme(), "http" | "https") && url.host_str().is_some() {
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

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("plugin HTTP runtime can be created")
}

fn read_utf8(ptr: *const u8, len: usize) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    std::str::from_utf8(unsafe { slice::from_raw_parts(ptr, len) })
        .ok()
        .map(ToOwned::to_owned)
}

fn parse_optional_json<T: DeserializeOwned>(ptr: *const u8, len: usize) -> Option<T> {
    let value = read_utf8(ptr, len)?;
    if value.trim().is_empty() {
        return None;
    }
    serde_json::from_str(&value).ok()
}

#[derive(Serialize)]
struct TicketExchangeRequest<'a> {
    ticket: &'a str,
}

#[derive(Serialize)]
struct EmptyRequest {}
