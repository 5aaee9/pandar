use axum::{
    Json,
    extract::rejection::JsonRejection,
    extract::{Path, State},
    http::StatusCode,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use pandar_core::{Job, JobArtifact, JobId};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    repositories::{CreatePrintJob, JobWithArtifact},
    routes::{ApiError, parse_tenant_id},
};

#[derive(Debug, Deserialize)]
pub struct CreateJobRequest {
    filename: String,
    content_type: String,
    artifact_base64: String,
    plate_id: u32,
    use_ams: bool,
    flow_cali: bool,
    timelapse: bool,
}

#[derive(Debug, Serialize)]
pub struct JobResponse {
    id: String,
    tenant_id: String,
    printer_id: String,
    agent_id: String,
    artifact_id: String,
    command_id: String,
    status: String,
    error: Option<String>,
    created_at: String,
    updated_at: String,
    command: JobCommandResponse,
    artifact: JobArtifactResponse,
}

#[derive(Debug, Serialize)]
pub struct JobArtifactResponse {
    id: String,
    tenant_id: String,
    filename: String,
    content_type: String,
    size_bytes: u64,
    storage_path: String,
    created_at: String,
}

#[derive(Debug, Serialize)]
pub struct JobCommandResponse {
    id: String,
    kind: &'static str,
    status: String,
}

#[derive(Debug, Serialize)]
pub struct JobListResponse {
    jobs: Vec<JobResponse>,
}

pub async fn create_job(
    State(state): State<AppState>,
    Path((tenant_id, printer_id)): Path<(String, String)>,
    payload: Result<Json<CreateJobRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<JobResponse>), ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    parse_printer_id(&printer_id)?;
    let Json(payload) = payload.map_err(|_| ApiError::bad_request("bad_request"))?;
    if payload.filename.trim().is_empty() || payload.artifact_base64.trim().is_empty() {
        return Err(ApiError::bad_request("bad_request"));
    }
    if payload.plate_id == 0 {
        return Err(ApiError::bad_request("invalid_plate_id"));
    }
    let content_type = if payload.content_type.trim().is_empty() {
        "application/octet-stream".to_string()
    } else {
        payload.content_type
    };

    let artifact_bytes = STANDARD
        .decode(payload.artifact_base64)
        .map_err(|_| ApiError::bad_request("invalid_artifact_base64"))?;
    if artifact_bytes.is_empty() {
        return Err(ApiError::bad_request("empty_artifact"));
    }
    if artifact_bytes.len() > state.job_storage().max_artifact_bytes() {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "artifact_too_large",
        ));
    }

    let Some(printer) = state
        .printers()
        .get_for_tenant(tenant_id, &printer_id)
        .await?
    else {
        return Err(ApiError::not_found("printer_not_found"));
    };
    let artifact_id = uuid::Uuid::new_v4().to_string();
    let stored = state
        .job_storage()
        .write_artifact(tenant_id, &artifact_id, &payload.filename, &artifact_bytes)
        .await
        .map_err(|err| {
            tracing::error!(error = %format!("{err:#}"), "failed to write print artifact");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
        })?;

    let created = state
        .jobs()
        .create_print_job(CreatePrintJob {
            tenant_id,
            printer_id: printer.id,
            agent_id: printer.agent_id,
            artifact_id,
            artifact_filename: stored.filename,
            artifact_content_type: content_type,
            artifact_size_bytes: stored.size_bytes,
            artifact_storage_path: stored.storage_path.clone(),
            plate_id: payload.plate_id,
            use_ams: payload.use_ams,
            flow_cali: payload.flow_cali,
            timelapse: payload.timelapse,
        })
        .await;

    match created {
        Ok(created) => Ok((StatusCode::CREATED, Json(JobResponse::from(created)))),
        Err(err) => {
            if let Err(cleanup_err) = state
                .job_storage()
                .remove_artifact(&stored.storage_path)
                .await
            {
                tracing::warn!(
                    error = %format!("{cleanup_err:#}"),
                    storage_path = %stored.storage_path,
                    "failed to remove print artifact after repository error"
                );
            }
            Err(err.into())
        }
    }
}

fn parse_printer_id(value: &str) -> Result<(), ApiError> {
    uuid::Uuid::parse_str(value).map_err(|_| ApiError::bad_request("invalid_printer_id"))?;
    Ok(())
}

pub async fn list_jobs(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
) -> Result<Json<JobListResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let jobs = state
        .jobs()
        .list_for_tenant(tenant_id)
        .await?
        .into_iter()
        .map(JobResponse::from)
        .collect();

    Ok(Json(JobListResponse { jobs }))
}

pub async fn get_job(
    State(state): State<AppState>,
    Path((tenant_id, job_id)): Path<(String, String)>,
) -> Result<Json<JobResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let job_id = JobId::parse(&job_id).map_err(|_| ApiError::bad_request("invalid_job_id"))?;
    let Some(job) = state.jobs().get_for_tenant(tenant_id, job_id).await? else {
        return Err(ApiError::not_found("job_not_found"));
    };

    Ok(Json(JobResponse::from(job)))
}

impl From<JobWithArtifact> for JobResponse {
    fn from(value: JobWithArtifact) -> Self {
        Self::from_parts(value.job, value.artifact)
    }
}

impl JobResponse {
    fn from_parts(job: Job, artifact: JobArtifact) -> Self {
        Self {
            id: job.id.to_string(),
            tenant_id: job.tenant_id.to_string(),
            printer_id: job.printer_id,
            agent_id: job.agent_id.to_string(),
            artifact_id: job.artifact_id,
            command_id: job.command_id.to_string(),
            status: job.status.to_string(),
            error: job.error,
            created_at: job.created_at,
            updated_at: job.updated_at,
            command: JobCommandResponse {
                id: job.command_id.to_string(),
                kind: "print_project_file",
                status: job.status.to_string(),
            },
            artifact: JobArtifactResponse::from(artifact),
        }
    }
}

impl From<JobArtifact> for JobArtifactResponse {
    fn from(artifact: JobArtifact) -> Self {
        Self {
            id: artifact.id,
            tenant_id: artifact.tenant_id.to_string(),
            filename: artifact.filename,
            content_type: artifact.content_type,
            size_bytes: artifact.size_bytes,
            storage_path: artifact.storage_path,
            created_at: artifact.created_at,
        }
    }
}
