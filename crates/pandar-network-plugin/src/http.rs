use anyhow::Context;
use futures_util::TryStreamExt;
use pandar_core::{PrintCalibrationMode, StudioAmsMappingEntry, StudioAmsMappingInfo};
use serde::{Deserialize, Serialize};
use std::{io::Write, path::PathBuf, sync::OnceLock, time::Duration};

use super::{
    PluginHttpResult, RequestKind, invalid_input, network_error, normalize_hub_url, read_utf8,
    result, runtime, stable_error_body,
};
use crate::cancellation::RequestCancellation;

pub(super) use response::read_bounded_response_body;

pub(super) mod cancellable;
mod diagnostics;
mod response;
#[cfg(test)]
mod tests;

#[derive(Serialize)]
pub(super) struct TicketExchangeRequest<'a> {
    pub(super) ticket: &'a str,
}

#[derive(Serialize)]
pub(super) struct EmptyRequest {}

const PLUGIN_SESSION_DELETE_TIMEOUT: Duration = Duration::from_secs(2);
const NO_AUTH_SESSION_POST_TIMEOUT: Duration = Duration::from_secs(5);
const PLUGIN_HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const PLUGIN_HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const PLUGIN_HTTP_MAX_RESPONSE_BYTES: usize = 1024 * 1024;

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(PLUGIN_HTTP_CONNECT_TIMEOUT)
            .timeout(PLUGIN_HTTP_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("plugin HTTP client configuration is valid")
    })
}

pub(super) fn calibration_mode(value: i32) -> Option<PrintCalibrationMode> {
    let value = u8::try_from(value).ok()?;
    PrintCalibrationMode::try_from(value).ok()
}

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
    diagnostics::buffered(|writer| {
        match runtime().block_on(async {
            execute_request(
                client().get(format!("{hub_url}{path}")).bearer_auth(token),
                None,
            )
            .await
            .context("GET plugin request")
        }) {
            Ok(response) => response_result_with_writer(response, kind, writer),
            Err(error) => {
                write_network_error(writer, &error);
                network_error()
            }
        }
    })
}

pub(super) fn post_json(
    url: &str,
    token: Option<&str>,
    body: impl Serialize,
    kind: RequestKind,
) -> PluginHttpResult {
    diagnostics::buffered(|writer| post_json_with_writer(url, token, body, kind, writer))
}

pub(super) fn post_json_with_writer<W: Write>(
    url: &str,
    token: Option<&str>,
    body: impl Serialize,
    kind: RequestKind,
    writer: &mut W,
) -> PluginHttpResult {
    match send_json(url, token, &body, kind, None) {
        Ok(response) => response_result_with_writer(response, kind, writer),
        Err(error) => {
            write_network_error(writer, &error);
            network_error()
        }
    }
}

#[cfg(test)]
fn post_json_with_connect_failure_with_writer(
    url: &str,
    body: impl Serialize,
    kind: RequestKind,
    writer: &mut impl Write,
) -> PluginHttpResult {
    cancellable::post_json_with_connect_failure_with_writer(
        url,
        body,
        kind,
        RequestCancellation::disabled(),
        writer,
    )
}

pub(super) fn delete_session(url: &str, token: &str, kind: RequestKind) -> PluginHttpResult {
    cancellable::delete_session(url, token, kind, RequestCancellation::disabled())
}

fn send_json(
    url: &str,
    token: Option<&str>,
    body: &impl Serialize,
    kind: RequestKind,
    timeout: Option<Duration>,
) -> anyhow::Result<HttpResponse> {
    runtime().block_on(async {
        let request = client().post(url).json(&body);
        let request = if let Some(token) = token {
            request.bearer_auth(token)
        } else {
            request
        };
        execute_request(request, timeout)
            .await
            .context(post_request_context(kind))
    })
}

struct HttpResponse {
    http_code: u32,
    body: anyhow::Result<String>,
}

async fn execute_request(
    request: reqwest::RequestBuilder,
    timeout: Option<Duration>,
) -> anyhow::Result<HttpResponse> {
    let response = match timeout {
        Some(timeout) => tokio::time::timeout(timeout, request.send())
            .await
            .context("plugin HTTP request timed out")?,
        None => request.send().await,
    };
    let response = response.map_err(reqwest::Error::without_url)?;
    let http_code = response.status().as_u16().into();
    let body = read_bounded_response_body(response);
    let body = match timeout {
        Some(timeout) => tokio::time::timeout(timeout, body)
            .await
            .context("plugin HTTP response body timed out")
            .and_then(|body| body),
        None => body.await,
    }
    .context("read plugin HTTP response body");
    Ok(HttpResponse { http_code, body })
}

fn post_request_context(kind: RequestKind) -> &'static str {
    match kind {
        RequestKind::TicketExchange => "POST plugin authentication request",
        RequestKind::PrinterLookup => "POST plugin printer lookup request",
        RequestKind::JobLookup => "POST plugin job lookup request",
        RequestKind::PrintSubmission => "POST plugin print submission request",
        RequestKind::PrinterOperation => "POST plugin printer operation request",
        RequestKind::PluginSession => "DELETE plugin session request",
    }
}

fn write_network_error(writer: &mut impl Write, error: &anyhow::Error) {
    let _ = writeln!(writer, "pandar network plugin request failed: {error:#}");
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
    pub(super) bed_leveling: bool,
    pub(super) auto_bed_leveling: PrintCalibrationMode,
    pub(super) flow_cali: bool,
    pub(super) auto_flow_cali: PrintCalibrationMode,
    pub(super) auto_offset_cali: PrintCalibrationMode,
    pub(super) timelapse: bool,
    pub(super) ams_mapping: Option<AmsMapping>,
    pub(super) ams_mapping2: Option<AmsMapping2>,
    pub(super) ams_mapping_info: Option<AmsMappingInfo>,
}

pub(super) type AmsMapping = Vec<i32>;
pub(super) type AmsMapping2 = Vec<StudioAmsMappingEntry>;
pub(super) type AmsMappingInfo = Vec<StudioAmsMappingInfo>;
enum PrintSubmissionError {
    LocalArtifact,
    Request(anyhow::Error),
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
    diagnostics::buffered(|writer| post_multipart_print_with_writer(url, token, body, writer))
}

fn post_multipart_print_with_writer(
    url: &str,
    token: &str,
    body: PrintSubmissionBody,
    writer: &mut impl Write,
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
        .map_err(anyhow::Error::from)
        .map_err(PrintSubmissionError::Request)?;
        let mut form = reqwest::multipart::Form::new()
            .text("printer_id", body.printer_id)
            .text("filename", body.filename)
            .text("content_type", "model/3mf")
            .text("plate_id", body.plate_id.to_string())
            .text("use_ams", body.use_ams.to_string())
            .text("bed_leveling", body.bed_leveling.to_string())
            .text(
                "auto_bed_leveling",
                body.auto_bed_leveling.as_u8().to_string(),
            )
            .text("flow_cali", body.flow_cali.to_string())
            .text("auto_flow_cali", body.auto_flow_cali.as_u8().to_string())
            .text(
                "auto_offset_cali",
                body.auto_offset_cali.as_u8().to_string(),
            )
            .text("timelapse", body.timelapse.to_string());
        if let Some(ams_mapping) = body.ams_mapping {
            form = form.text("ams_mapping", multipart_json_text(&ams_mapping));
        }
        if let Some(ams_mapping2) = body.ams_mapping2 {
            form = form.text("ams_mapping2", multipart_json_text(&ams_mapping2));
        }
        if let Some(ams_mapping_info) = body.ams_mapping_info {
            form = form.text("ams_mapping_info", multipart_json_text(&ams_mapping_info));
        }
        let request = client()
            .post(url)
            .bearer_auth(token)
            .multipart(form.part("file", file));
        execute_request(request, None)
            .await
            .context("POST plugin multipart print submission request")
            .map_err(PrintSubmissionError::Request)
    }) {
        Ok(response) => response_result_with_writer(response, RequestKind::PrintSubmission, writer),
        Err(PrintSubmissionError::LocalArtifact) => invalid_input("artifact_missing"),
        Err(PrintSubmissionError::Request(error)) => {
            write_network_error(writer, &error);
            network_error()
        }
    }
}

fn multipart_json_text(value: &impl Serialize) -> String {
    serde_json::to_string(value).expect("multipart mapping payload is serializable")
}

fn response_result_with_writer(
    response: HttpResponse,
    kind: RequestKind,
    writer: &mut impl Write,
) -> PluginHttpResult {
    let HttpResponse { http_code, body } = response;
    match body {
        Ok(body) => {
            if (200..300).contains(&http_code) {
                result(0, http_code, body)
            } else {
                result(1, http_code, redact_hub_error(kind, http_code, &body))
            }
        }
        Err(error) => {
            write_network_error(writer, &error);
            result(1, http_code, stable_error_body("invalid_response"))
        }
    }
}

pub(super) fn redact_hub_error(kind: RequestKind, http_code: u32, body: &str) -> String {
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
            | "ambiguous_no_auth_tenant"
            | "tenant_not_found"
    )
}
