use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

use super::{
    PluginHttpResult, RequestKind, invalid_input, network_error, normalize_hub_url, read_utf8,
    result, runtime, stable_error_body,
};

pub(super) fn get_json(
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

pub(super) fn post_json(
    url: &str,
    token: Option<&str>,
    body: impl Serialize,
    kind: RequestKind,
) -> PluginHttpResult {
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

pub(super) fn plugin_printer_operation_url(
    hub_url: &str,
    printer_id: &str,
) -> Option<reqwest::Url> {
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

pub(super) struct PrintSubmissionBody {
    pub(super) printer_id: String,
    pub(super) filename: String,
    pub(super) artifact_path: PathBuf,
    pub(super) artifact_len: u64,
    pub(super) plate_id: i64,
    pub(super) use_ams: bool,
    pub(super) flow_cali: bool,
    pub(super) timelapse: bool,
    pub(super) ams_mapping: Option<Value>,
    pub(super) ams_mapping2: Option<Value>,
    pub(super) ams_mapping_info: Option<Value>,
}

enum PrintSubmissionError {
    LocalArtifact,
    Request,
}

#[derive(Deserialize)]
struct HubErrorBody {
    error: Option<String>,
}

pub(super) fn post_multipart_print(
    url: &str,
    token: &str,
    body: PrintSubmissionBody,
) -> PluginHttpResult {
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
        let mut form = reqwest::multipart::Form::new()
            .text("printer_id", body.printer_id)
            .text("filename", body.filename)
            .text("content_type", "model/3mf")
            .text("plate_id", body.plate_id.to_string())
            .text("use_ams", body.use_ams.to_string())
            .text("flow_cali", body.flow_cali.to_string())
            .text("timelapse", body.timelapse.to_string());
        if let Some(ams_mapping) = body.ams_mapping {
            form = form.text("ams_mapping", ams_mapping.to_string());
        }
        if let Some(ams_mapping2) = body.ams_mapping2 {
            form = form.text("ams_mapping2", ams_mapping2.to_string());
        }
        if let Some(ams_mapping_info) = body.ams_mapping_info {
            form = form.text("ams_mapping_info", ams_mapping_info.to_string());
        }
        let request = reqwest::Client::new()
            .post(url)
            .bearer_auth(token)
            .multipart(form.part("file", file));
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
    let hub_error = serde_json::from_str::<HubErrorBody>(body)
        .ok()
        .and_then(|body| body.error);
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
            | "invalid_printer_id"
            | "plugin_forbidden"
    )
}
