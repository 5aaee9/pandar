use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use futures_util::StreamExt;
use http_body::Frame;
use http_body_util::StreamBody;
use reqwest::{Client, Method, multipart};
use serde::{Deserialize, Serialize};
use tokio_util::io::ReaderStream;

use super::{
    admission::{AdmittedPrint, PrintFailure},
    diagnostics::diagnose_json,
    ffi::PluginStudioCallbacks,
};

const UPLOAD_CHUNK_BYTES: usize = 64 * 1024;

pub(super) struct HttpReply {
    pub(super) status: u16,
    pub(super) body: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HubError {
    error: String,
}

pub(super) fn client() -> Result<Client, PrintFailure> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|error| diagnosed_failure(error.into(), "build Studio print HTTP client"))
}

pub(super) async fn request(
    client: &Client,
    method: Method,
    url: String,
    token: &str,
) -> Result<HttpReply, PrintFailure> {
    let response = client
        .request(method, url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| {
            diagnosed_failure(error.without_url().into(), "send Studio print Hub request")
        })?;
    reply(response).await
}

pub(super) async fn submit(
    client: &Client,
    print: &AdmittedPrint,
    callbacks: PluginStudioCallbacks,
) -> Result<HttpReply, PrintFailure> {
    let file = tokio::fs::File::open(&print.artifact_path)
        .await
        .map_err(|error| diagnosed_artifact_failure(error, "open Studio print artifact"))?;
    let artifact_len = file
        .metadata()
        .await
        .map_err(|error| diagnosed_artifact_failure(error, "inspect Studio print artifact"))?
        .len();
    if artifact_len == 0 {
        return Err(PrintFailure::simple("artifact_empty"));
    }

    let cancelled = Arc::new(AtomicBool::new(false));
    let stream_cancelled = Arc::clone(&cancelled);
    let sent = Arc::new(AtomicU64::new(0));
    let stream_sent = Arc::clone(&sent);
    let stream = ReaderStream::with_capacity(file, UPLOAD_CHUNK_BYTES).map(move |chunk| {
        if callbacks.cancelled() {
            stream_cancelled.store(true, Ordering::Release);
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Studio print upload cancelled",
            ));
        }
        let bytes = chunk?;
        let uploaded = stream_sent
            .fetch_add(bytes.len() as u64, Ordering::AcqRel)
            .saturating_add(bytes.len() as u64);
        let progress = uploaded
            .saturating_mul(100)
            .checked_div(artifact_len)
            .unwrap_or(0);
        callbacks.update(1, i32::try_from(progress.min(100)).unwrap_or(100), "");
        Ok(Frame::data(bytes))
    });
    let part = multipart::Part::stream_with_length(
        reqwest::Body::wrap(StreamBody::new(stream)),
        artifact_len,
    )
    .file_name(print.artifact_filename.clone())
    .mime_str("model/3mf")
    .map_err(|_| PrintFailure::simple("invalid_print_param"))?;
    let form = multipart_form(print).part("file", part);
    let response = client
        .post(format!("{}/api/v1/plugin/prints", print.hub_url))
        .bearer_auth(&print.token)
        .multipart(form)
        .send()
        .await;
    if cancelled.load(Ordering::Acquire) {
        return Err(PrintFailure::cancelled());
    }
    let response = response.map_err(|error| {
        diagnosed_failure(error.without_url().into(), "upload Studio print artifact")
    })?;
    reply(response).await
}

fn multipart_form(print: &AdmittedPrint) -> multipart::Form {
    let mut form = multipart::Form::new()
        .text("printer_id", print.printer_id.clone())
        .text("filename", print.artifact_filename.clone())
        .text("content_type", "model/3mf")
        .text("plate_id", print.plate_index.to_string())
        .text("task_name", print.task_name.clone())
        .text("project_name", print.project_name.clone())
        .text("preset_name", print.preset_name.clone())
        .text("nozzle_mapping", json_text(&print.nozzle_mapping))
        .text("ams_mapping", json_text(&print.ams_mapping))
        .text("ams_mapping2", json_text(&print.ams_mapping2))
        .text("ams_mapping_info", json_text(&print.ams_mapping_info))
        .text("nozzles_info", json_text(&print.nozzles_info))
        .text("connection_type", print.connection_type.clone())
        .text("comments", print.comments.clone())
        .text("origin_profile_id", print.origin_profile_id.to_string())
        .text("stl_design_id", print.stl_design_id.to_string())
        .text("origin_model_id", print.origin_model_id.clone())
        .text("print_type", print.print_type.clone())
        .text("dev_name", print.dev_name.clone())
        .text("bed_leveling", print.bed_leveling.to_string())
        .text("flow_cali", print.flow_cali.to_string())
        .text("vibration_cali", print.vibration_cali.to_string())
        .text("layer_inspect", print.layer_inspect.to_string())
        .text("timelapse", print.timelapse.to_string())
        .text(
            "timelapse_use_internal",
            print.timelapse_use_internal.to_string(),
        )
        .text("use_ams", print.use_ams.to_string())
        .text("bed_type", print.bed_type.clone())
        .text(
            "auto_bed_leveling",
            print.auto_bed_leveling.as_u8().to_string(),
        )
        .text("auto_flow_cali", print.auto_flow_cali.as_u8().to_string())
        .text(
            "auto_offset_cali",
            print.auto_offset_cali.as_u8().to_string(),
        )
        .text(
            "extruder_cali_manual_mode",
            print.extruder_cali_manual_mode.to_string(),
        )
        .text("try_emmc_print", print.try_emmc_print.to_string())
        .text("svc_context", print.svc_context.clone())
        .text("slicer_uid", print.slicer_uid.clone());
    if let Some(config_plate_index) = print.config_plate_index {
        form = form.text("config_plate_index", config_plate_index.to_string());
    }
    form
}

async fn reply(response: reqwest::Response) -> Result<HttpReply, PrintFailure> {
    let status = response.status().as_u16();
    let body = response.text().await.map_err(|error| {
        let error =
            anyhow::Error::new(error.without_url()).context("read Studio print Hub response");
        eprintln!("pandar network plugin request failed: {error:#}");
        PrintFailure::simple("invalid_response")
    })?;
    Ok(HttpReply { status, body })
}

pub(super) fn failure_from_reply(reply: &HttpReply) -> PrintFailure {
    let error = serde_json::from_str::<HubError>(&reply.body)
        .map(|body| body.error)
        .map_err(|error| {
            diagnose_json(&error, "decode Studio print Hub error response");
        })
        .ok()
        .filter(|error| stable_hub_error(error))
        .unwrap_or_else(|| match reply.status {
            401 => "invalid_auth_token".to_owned(),
            403 => "plugin_forbidden".to_owned(),
            404 => "task_not_found".to_owned(),
            _ => "invalid_response".to_owned(),
        });
    PrintFailure {
        code: -19,
        body: serde_json::to_string(&SerializableError { error: &error })
            .expect("Hub error body is serializable"),
    }
}

#[derive(Serialize)]
struct SerializableError<'a> {
    error: &'a str,
}

fn stable_hub_error(error: &str) -> bool {
    matches!(
        error,
        "artifact_empty"
            | "artifact_invalid_plate"
            | "artifact_invalid_upload"
            | "artifact_too_large"
            | "cancel_too_late"
            | "command_cancelled"
            | "invalid_auth_token"
            | "invalid_printer_id"
            | "invalid_studio_submission_id"
            | "invalid_task_id"
            | "invalid_task_query"
            | "invalid_task_pagination"
            | "invalid_task_status"
            | "job_cancelled"
            | "job_failed"
            | "job_not_found"
            | "hub_unavailable"
            | "plate_unavailable"
            | "plugin_forbidden"
            | "plugin_token_revoked"
            | "printer_not_found"
            | "slice_info_unavailable"
            | "studio_model_task_metadata_unavailable"
            | "studio_task_metadata_unavailable"
            | "subtask_unavailable"
            | "task_not_found"
            | "task_unavailable"
    )
}

fn json_text(value: &impl Serialize) -> String {
    serde_json::to_string(value).expect("admitted Studio metadata is serializable")
}

fn diagnosed_failure(error: anyhow::Error, context: &'static str) -> PrintFailure {
    let error = error.context(context);
    eprintln!("pandar network plugin request failed: {error:#}");
    PrintFailure::simple("hub_unavailable")
}

fn diagnosed_artifact_failure(error: std::io::Error, context: &'static str) -> PrintFailure {
    let error = anyhow::Error::new(error).context(context);
    eprintln!("pandar network plugin artifact failed: {error:#}");
    PrintFailure::simple("artifact_missing")
}
