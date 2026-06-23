use std::{ffi::c_void, slice};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

pub const PLUGIN_NAME: &str = "pandar-network-plugin";

#[repr(C)]
pub struct PluginHttpResult {
    pub status: i32,
    pub http_code: u32,
    pub body_ptr: *mut u8,
    pub body_len: usize,
    pub body_cap: usize,
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_exchange_ticket(
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    ticket_ptr: *const u8,
    ticket_len: usize,
) -> PluginHttpResult {
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len) else {
        return invalid_input("invalid_hub_url");
    };
    let Some(ticket) = read_utf8(ticket_ptr, ticket_len).filter(|ticket| !ticket.trim().is_empty())
    else {
        return invalid_input("invalid_plugin_ticket");
    };
    post_json(
        &format!(
            "{}/api/v1/plugin/login-tickets/exchange",
            trim_slash(hub_url)
        ),
        None,
        json!({ "ticket": ticket }),
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
) -> PluginHttpResult {
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len) else {
        return invalid_input("invalid_hub_url");
    };
    let Some(token) = read_utf8(token_ptr, token_len) else {
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
    let artifact = match std::fs::read(artifact_path) {
        Ok(bytes) => bytes,
        Err(_) => return invalid_input("artifact_missing"),
    };
    if artifact.is_empty() {
        return invalid_input("artifact_empty");
    }
    let ams_mapping = parse_optional_json(ams_mapping_ptr, ams_mapping_len);
    let ams_mapping2 = parse_optional_json(ams_mapping2_ptr, ams_mapping2_len);

    post_json(
        &format!("{}/api/v1/plugin/prints", trim_slash(hub_url)),
        Some(&token),
        json!({
            "printer_id": printer_id,
            "filename": filename,
            "content_type": "model/3mf",
            "artifact_base64": STANDARD.encode(artifact),
            "plate_id": plate_id,
            "use_ams": use_ams,
            "flow_cali": flow_cali,
            "timelapse": timelapse,
            "ams_mapping": ams_mapping,
            "ams_mapping2": ams_mapping2,
        }),
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

fn get_json(
    hub_url_ptr: *const u8,
    hub_url_len: usize,
    token_ptr: *const u8,
    token_len: usize,
    path: &str,
) -> PluginHttpResult {
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len) else {
        return invalid_input("invalid_hub_url");
    };
    let Some(token) = read_utf8(token_ptr, token_len) else {
        return invalid_input("invalid_auth_token");
    };
    match runtime().block_on(async {
        reqwest::Client::new()
            .get(format!("{}{}", trim_slash(hub_url), path))
            .bearer_auth(token)
            .send()
            .await
    }) {
        Ok(response) => response_result(response),
        Err(_) => network_error(),
    }
}

fn post_json(url: &str, token: Option<&str>, body: Value) -> PluginHttpResult {
    match runtime().block_on(async {
        let request = reqwest::Client::new().post(url).json(&body);
        let request = if let Some(token) = token {
            request.bearer_auth(token)
        } else {
            request
        };
        request.send().await
    }) {
        Ok(response) => response_result(response),
        Err(_) => network_error(),
    }
}

fn response_result(response: reqwest::Response) -> PluginHttpResult {
    let http_code = response.status().as_u16().into();
    match runtime().block_on(response.text()) {
        Ok(body) => {
            let status = if (200..300).contains(&http_code) {
                0
            } else {
                1
            };
            result(status, http_code, body)
        }
        Err(_) => result(1, http_code, r#"{"error":"invalid_response"}"#),
    }
}

fn invalid_input(error: &str) -> PluginHttpResult {
    result(1, 400, format!(r#"{{"error":"{error}"}}"#))
}

fn network_error() -> PluginHttpResult {
    result(1, 0, r#"{"error":"hub_unavailable"}"#)
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

fn trim_slash(value: String) -> String {
    value.trim_end_matches('/').to_string()
}

fn parse_optional_json(ptr: *const u8, len: usize) -> Option<Value> {
    let value = read_utf8(ptr, len)?;
    if value.trim().is_empty() {
        return None;
    }
    serde_json::from_str(&value).ok()
}
