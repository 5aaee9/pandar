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
        .and_then(|body| serde_json::from_str::<Value>(&body).ok())
        .filter(valid_operation_json)
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
        Some(operation) => result(0, 200, operation.to_string()),
        None => invalid_input("unsupported_printer_operation"),
    }
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

fn plugin_printer_operation_url(hub_url: &str, printer_id: &str) -> Option<reqwest::Url> {
    let mut url = reqwest::Url::parse(hub_url).ok()?;
    url.path_segments_mut().ok()?.extend([
        "api",
        "v1",
        "plugin",
        "printers",
        printer_id,
        "operations",
    ]);
    Some(url)
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
                RequestKind::PrinterLookup
                    | RequestKind::JobLookup
                    | RequestKind::PrintSubmission
                    | RequestKind::PrinterOperation
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
            | "printer_operation_unavailable"
            | "unsupported_printer_operation"
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

fn operation_json_from_gcode(message: &str) -> Option<Value> {
    let commands = gcode_commands(message);
    match commands.as_slice() {
        [command] => parse_single_command_operation(command),
        [relative, movement] if relative.eq_ignore_ascii_case("G91") => {
            parse_move_axes_operation(movement)
        }
        _ => None,
    }
}

fn valid_operation_json(operation: &Value) -> bool {
    let Some(action) = operation.get("action").and_then(Value::as_str) else {
        return false;
    };

    match action {
        "pause" | "resume" | "stop" => operation
            .as_object()
            .is_some_and(|object| object.len() == 1),
        "set_print_speed" => {
            operation
                .get("speed_mode")
                .and_then(Value::as_u64)
                .is_some_and(|speed| (1..=4).contains(&speed))
                && operation
                    .as_object()
                    .is_some_and(|object| object.len() == 2)
        }
        "home" => {
            operation
                .get("axes")
                .and_then(Value::as_array)
                .is_some_and(|axes| axes.iter().all(valid_axis_value))
                && operation
                    .as_object()
                    .is_some_and(|object| object.len() == 2)
        }
        "move_axes" => {
            let Some(movements) = operation.get("movements").and_then(Value::as_array) else {
                return false;
            };
            !movements.is_empty()
                && movements.iter().all(valid_movement_value)
                && operation
                    .get("feedrate_mm_per_min")
                    .is_none_or(valid_feedrate_value)
                && operation.as_object().is_some_and(|object| {
                    object.len()
                        == if operation.get("feedrate_mm_per_min").is_some() {
                            3
                        } else {
                            2
                        }
                })
        }
        "set_hotend_temperature" => {
            operation
                .get("temperature_celsius")
                .and_then(Value::as_u64)
                .is_some_and(|temperature| temperature <= 300)
                && operation.get("wait").is_none_or(Value::is_boolean)
                && operation.as_object().is_some_and(|object| {
                    object.len()
                        == if operation.get("wait").is_some() {
                            3
                        } else {
                            2
                        }
                })
        }
        _ => false,
    }
}

fn valid_axis_value(axis: &Value) -> bool {
    matches!(axis.as_str(), Some("x" | "y" | "z"))
}

fn valid_movement_value(movement: &Value) -> bool {
    let Some(object) = movement.as_object() else {
        return false;
    };
    object.len() == 2
        && movement.get("axis").is_some_and(valid_axis_value)
        && movement
            .get("delta_mm")
            .and_then(Value::as_f64)
            .is_some_and(|delta| delta.is_finite() && delta != 0.0 && delta.abs() <= 50.0)
}

fn valid_feedrate_value(feedrate: &Value) -> bool {
    feedrate
        .as_u64()
        .is_some_and(|feedrate| (1..=12_000).contains(&feedrate))
}

fn gcode_commands(message: &str) -> Vec<String> {
    message
        .lines()
        .filter_map(|line| {
            let command = line
                .split_once(';')
                .map_or(line, |(command, _)| command)
                .trim();
            (!command.is_empty()).then(|| command.to_owned())
        })
        .collect()
}

fn parse_single_command_operation(command: &str) -> Option<Value> {
    match command_code(command)? {
        "G28" => parse_home_operation(command),
        "M104" => parse_hotend_operation(command, false),
        "M109" => parse_hotend_operation(command, true),
        _ => None,
    }
}

fn parse_home_operation(command: &str) -> Option<Value> {
    let mut axes = Vec::new();
    for token in command.split_whitespace().skip(1) {
        let axis = match token.to_ascii_uppercase().as_str() {
            "X" => "x",
            "Y" => "y",
            "Z" => "z",
            _ => return None,
        };
        axes.push(axis);
    }
    Some(json!({ "action": "home", "axes": axes }))
}

fn parse_hotend_operation(command: &str, wait: bool) -> Option<Value> {
    let mut celsius = None;
    for token in command.split_whitespace().skip(1) {
        let mut chars = token.chars();
        let parameter = chars.next()?.to_ascii_uppercase();
        let value = parse_gcode_number(chars.as_str())?;
        match parameter {
            'S' if celsius.is_none() => celsius = Some(value),
            _ => return None,
        }
    }
    let celsius = parse_integer_gcode_value(celsius?)?;
    Some(json!({
        "action": "set_hotend_temperature",
        "temperature_celsius": celsius,
        "wait": wait,
    }))
}

fn parse_move_axes_operation(command: &str) -> Option<Value> {
    if !matches!(command_code(command)?, "G0" | "G1") {
        return None;
    }

    let mut movements = Vec::new();
    let mut feedrate = None;
    for token in command.split_whitespace().skip(1) {
        let mut chars = token.chars();
        let parameter = chars.next()?.to_ascii_uppercase();
        let value = parse_gcode_number(chars.as_str())?;
        match parameter {
            'X' | 'Y' | 'Z'
                if !movements.iter().any(|movement: &Value| {
                    movement["axis"] == parameter.to_ascii_lowercase().to_string()
                }) =>
            {
                movements.push(json!({
                    "axis": parameter.to_ascii_lowercase().to_string(),
                    "delta_mm": value,
                }));
            }
            'F' if feedrate.is_none() => feedrate = Some(value),
            _ => return None,
        }
    }
    if movements.is_empty() {
        return None;
    }

    let mut body = serde_json::Map::new();
    body.insert("action".to_string(), json!("move_axes"));
    body.insert("movements".to_string(), Value::Array(movements));
    if let Some(value) = feedrate {
        body.insert(
            "feedrate_mm_per_min".to_string(),
            json!(parse_integer_gcode_value(value)?),
        );
    }
    Some(Value::Object(body))
}

fn command_code(command: &str) -> Option<&str> {
    command
        .split_whitespace()
        .next()
        .map(|code| code.trim())
        .filter(|code| !code.is_empty())
        .and_then(|code| {
            if code.eq_ignore_ascii_case("G0") {
                Some("G0")
            } else if code.eq_ignore_ascii_case("G1") {
                Some("G1")
            } else if code.eq_ignore_ascii_case("G28") {
                Some("G28")
            } else if code.eq_ignore_ascii_case("G90") {
                Some("G90")
            } else if code.eq_ignore_ascii_case("G91") {
                Some("G91")
            } else if code.eq_ignore_ascii_case("M104") {
                Some("M104")
            } else if code.eq_ignore_ascii_case("M109") {
                Some("M109")
            } else {
                None
            }
        })
}

fn parse_gcode_number(value: &str) -> Option<f64> {
    (!value.is_empty())
        .then(|| value.parse::<f64>().ok())
        .flatten()
        .filter(|value| value.is_finite())
}

fn parse_integer_gcode_value(value: f64) -> Option<u32> {
    (value >= 0.0 && value.fract() == 0.0 && value <= u32::MAX as f64).then_some(value as u32)
}
