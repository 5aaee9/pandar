use std::{ffi::c_void, path::PathBuf, slice};

use futures_util::TryStreamExt;
use serde_json::{Value, json};

pub const PLUGIN_NAME: &str = "pandar-network-plugin";

#[derive(Clone, Copy)]
enum RequestKind {
    TicketExchange,
    PrinterLookup,
    JobLookup,
    PrintSubmission,
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
        json!({ "ticket": ticket }),
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
    let ams_mapping = parse_optional_json(ams_mapping_ptr, ams_mapping_len);
    let ams_mapping2 = parse_optional_json(ams_mapping2_ptr, ams_mapping2_len);

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
        },
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
    kind: RequestKind,
) -> PluginHttpResult {
    let Some(hub_url) = read_utf8(hub_url_ptr, hub_url_len).and_then(normalize_hub_url) else {
        return invalid_input("invalid_hub_url");
    };
    let Some(token) = read_utf8(token_ptr, token_len).filter(|token| !token.trim().is_empty())
    else {
        return invalid_input("invalid_auth_token");
    };
    match runtime().block_on(async {
        reqwest::Client::new()
            .get(format!("{hub_url}{path}"))
            .bearer_auth(token)
            .send()
            .await
    }) {
        Ok(response) => response_result(response, kind),
        Err(_) => network_error(),
    }
}

fn post_json(url: &str, token: Option<&str>, body: Value, kind: RequestKind) -> PluginHttpResult {
    match runtime().block_on(async {
        let request = reqwest::Client::new().post(url).json(&body);
        let request = if let Some(token) = token {
            request.bearer_auth(token)
        } else {
            request
        };
        request.send().await
    }) {
        Ok(response) => response_result(response, kind),
        Err(_) => network_error(),
    }
}

struct PrintSubmissionBody {
    printer_id: String,
    filename: String,
    artifact_path: PathBuf,
    artifact_len: u64,
    plate_id: i64,
    use_ams: bool,
    flow_cali: bool,
    timelapse: bool,
    ams_mapping: Option<Value>,
    ams_mapping2: Option<Value>,
}

enum PrintSubmissionError {
    LocalArtifact,
    Request,
}

fn post_multipart_print(url: &str, token: &str, body: PrintSubmissionBody) -> PluginHttpResult {
    match runtime().block_on(async {
        let artifact = tokio::fs::File::open(&body.artifact_path)
            .await
            .map_err(|_| PrintSubmissionError::LocalArtifact)?;
        let artifact_stream =
            tokio_util::io::ReaderStream::new(artifact).map_ok(http_body::Frame::data);
        let file = reqwest::multipart::Part::stream_with_length(
            reqwest::Body::wrap(http_body_util::StreamBody::new(artifact_stream)),
            body.artifact_len,
        )
        .file_name(body.filename.clone())
        .mime_str("model/3mf")
        .map_err(|_| PrintSubmissionError::Request)?;
        let request = reqwest::Client::new()
            .post(url)
            .bearer_auth(token)
            .multipart(
                reqwest::multipart::Form::new()
                    .text("printer_id", body.printer_id)
                    .text("filename", body.filename)
                    .text("content_type", "model/3mf")
                    .text("plate_id", body.plate_id.to_string())
                    .text("use_ams", body.use_ams.to_string())
                    .text("flow_cali", body.flow_cali.to_string())
                    .text("timelapse", body.timelapse.to_string())
                    .text(
                        "ams_mapping",
                        body.ams_mapping
                            .map_or_else(|| "null".to_string(), |value| value.to_string()),
                    )
                    .text(
                        "ams_mapping2",
                        body.ams_mapping2
                            .map_or_else(|| "null".to_string(), |value| value.to_string()),
                    )
                    .part("file", file),
            );
        request
            .send()
            .await
            .map_err(|_| PrintSubmissionError::Request)
    }) {
        Ok(response) => response_result(response, RequestKind::PrintSubmission),
        Err(PrintSubmissionError::LocalArtifact) => invalid_input("artifact_missing"),
        Err(PrintSubmissionError::Request) => network_error(),
    }
}

fn response_result(response: reqwest::Response, kind: RequestKind) -> PluginHttpResult {
    let http_code = response.status().as_u16().into();
    match runtime().block_on(response.text()) {
        Ok(body) => {
            if (200..300).contains(&http_code) {
                result(0, http_code, body)
            } else {
                result(1, http_code, redact_hub_error(kind, http_code, &body))
            }
        }
        Err(_) => result(1, http_code, stable_error_body("invalid_response")),
    }
}

fn redact_hub_error(kind: RequestKind, http_code: u32, body: &str) -> String {
    let hub_error = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|body| body.get("error").and_then(Value::as_str).map(str::to_owned));
    let error = match (http_code, hub_error.as_deref()) {
        (401, _) if matches!(kind, RequestKind::TicketExchange) => "invalid_plugin_ticket",
        (401, _) => "invalid_auth_token",
        (403, _) => "plugin_forbidden",
        (410, _) | (_, Some("token_revoked")) => "plugin_token_revoked",
        (_, Some(error)) if is_stable_hub_error(error) => error,
        (404, _)
            if matches!(
                kind,
                RequestKind::PrinterLookup | RequestKind::JobLookup | RequestKind::PrintSubmission
            ) =>
        {
            "printer_not_found"
        }
        _ => "invalid_response",
    };
    stable_error_body(error)
}

fn is_stable_hub_error(error: &str) -> bool {
    matches!(
        error,
        "artifact_invalid_plate"
            | "artifact_invalid_upload"
            | "artifact_too_large"
            | "printer_not_found"
            | "invalid_plugin_ticket"
            | "invalid_auth_token"
            | "plugin_forbidden"
    )
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

fn parse_optional_json(ptr: *const u8, len: usize) -> Option<Value> {
    let value = read_utf8(ptr, len)?;
    if value.trim().is_empty() {
        return None;
    }
    serde_json::from_str(&value).ok()
}
