use axum::{
    Json,
    extract::Multipart,
    extract::rejection::JsonRejection,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use pandar_core::{Job, JobArtifact, JobId, JobPrintState};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    AppState,
    repositories::{DuplicatePrintJob, JobWithArtifact, RepositoryError, UserRole},
    routes::{ApiError, auth, parse_tenant_id},
};

mod material;
mod metadata_preview;
pub(super) mod multipart;

#[derive(Debug, Deserialize)]
pub struct RecoveryReasonRequest {
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DuplicateJobRequest {
    printer_id: Option<String>,
    plate_id: Option<i64>,
    use_ams: Option<bool>,
    flow_cali: Option<bool>,
    timelapse: Option<bool>,
    ams_mapping: Option<Value>,
    ams_mapping2: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    print: JobPrintResponse,
    command: JobCommandResponse,
    artifact: JobArtifactResponse,
    material: material::JobMaterialResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobPrintResponse {
    status: String,
    printer_state: Option<String>,
    progress_percent: Option<u8>,
    remaining_time_minutes: Option<u32>,
    current_layer: Option<u32>,
    total_layers: Option<u32>,
    active_file: Option<String>,
    last_progress_percent: Option<u8>,
    last_layer: Option<u32>,
    error: Option<String>,
    started_at: Option<String>,
    finished_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobArtifactResponse {
    id: String,
    tenant_id: String,
    filename: String,
    content_type: String,
    size_bytes: u64,
    metadata: Option<Value>,
    created_at: String,
}

#[derive(Debug, Serialize)]
pub struct ArtifactMetadataPreviewResponse {
    metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobCommandResponse {
    id: String,
    kind: String,
    status: String,
}

#[derive(Debug, Serialize)]
pub struct JobListResponse {
    pub(in crate::routes) jobs: Vec<JobResponse>,
}
pub async fn create_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, printer_id)): Path<(String, String)>,
    multipart: Multipart,
) -> Result<(StatusCode, Json<JobResponse>), ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let auth =
        auth::authorize_tenant_principal(&state, &headers, tenant_id, UserRole::Operator).await?;
    parse_printer_id(&printer_id)?;
    let created = multipart::create_print_job_from_multipart(
        &state,
        tenant_id,
        Some(printer_id),
        multipart,
        auth::audit_actor(&auth),
        "print",
    )
    .await?;
    let wake_tenant_id = created.job.tenant_id;
    let wake_agent_id = created.job.agent_id;
    let response = JobResponse::try_from(created)?;
    state.wake_agent(wake_tenant_id, wake_agent_id).await;
    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn preview_artifact_metadata(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    multipart: Multipart,
) -> Result<Json<ArtifactMetadataPreviewResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant_principal(&state, &headers, tenant_id, UserRole::Operator).await?;
    let metadata =
        metadata_preview::preview_artifact_metadata_from_multipart(&state, multipart).await?;
    Ok(Json(ArtifactMetadataPreviewResponse { metadata }))
}

pub async fn retry_dispatch(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, job_id)): Path<(String, String)>,
    payload: Result<Json<RecoveryReasonRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<JobResponse>), ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let auth =
        auth::authorize_tenant_principal(&state, &headers, tenant_id, UserRole::Operator).await?;
    let job_id = JobId::parse(&job_id).map_err(|_| ApiError::bad_request("invalid_job_id"))?;
    let Json(payload) = payload.map_err(|_| ApiError::bad_request("bad_request"))?;
    let reason = payload.reason;
    let job = state
        .jobs()
        .retry_dispatch_with_audit(tenant_id, job_id, reason, auth::audit_actor(&auth))
        .await?;
    let wake_tenant_id = job.job.tenant_id;
    let wake_agent_id = job.job.agent_id;
    let response = JobResponse::try_from(job)?;
    state.wake_agent(wake_tenant_id, wake_agent_id).await;
    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn reprint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, job_id)): Path<(String, String)>,
    payload: Result<Json<RecoveryReasonRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<JobResponse>), ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let auth =
        auth::authorize_tenant_principal(&state, &headers, tenant_id, UserRole::Operator).await?;
    let job_id = JobId::parse(&job_id).map_err(|_| ApiError::bad_request("invalid_job_id"))?;
    let Json(payload) = payload.map_err(|_| ApiError::bad_request("bad_request"))?;
    let reason = payload.reason;
    let job = state
        .jobs()
        .reprint_with_audit(tenant_id, job_id, reason, auth::audit_actor(&auth))
        .await?;
    let wake_tenant_id = job.job.tenant_id;
    let wake_agent_id = job.job.agent_id;
    let response = JobResponse::try_from(job)?;
    state.wake_agent(wake_tenant_id, wake_agent_id).await;
    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn duplicate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, job_id)): Path<(String, String)>,
    payload: Result<Json<DuplicateJobRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<JobResponse>), ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let auth =
        auth::authorize_tenant_principal(&state, &headers, tenant_id, UserRole::Operator).await?;
    let job_id = JobId::parse(&job_id).map_err(|_| ApiError::bad_request("invalid_job_id"))?;
    let Json(payload) = payload.map_err(|_| ApiError::bad_request("bad_request"))?;
    let plate_id = payload.plate_id.map(validated_plate_id).transpose()?;
    if let Some(printer_id) = &payload.printer_id {
        parse_printer_id(printer_id)?;
    }
    let job = state
        .jobs()
        .duplicate_and_print_with_audit(
            tenant_id,
            job_id,
            DuplicatePrintJob {
                printer_id: payload.printer_id,
                plate_id,
                use_ams: payload.use_ams,
                flow_cali: payload.flow_cali,
                timelapse: payload.timelapse,
                ams_mapping_json: material::mapping_json(payload.ams_mapping, "ams_mapping")?,
                ams_mapping2_json: material::mapping_json(payload.ams_mapping2, "ams_mapping2")?,
            },
            auth::audit_actor(&auth),
        )
        .await?;
    let wake_tenant_id = job.job.tenant_id;
    let wake_agent_id = job.job.agent_id;
    let response = JobResponse::try_from(job)?;
    state.wake_agent(wake_tenant_id, wake_agent_id).await;
    Ok((StatusCode::CREATED, Json(response)))
}

pub(super) fn redact_artifact_error(message: &str) -> String {
    crate::routes::plugin::redact_artifact_error(message)
}

pub(super) fn parse_printer_id(value: &str) -> Result<(), ApiError> {
    uuid::Uuid::parse_str(value).map_err(|_| ApiError::bad_request("invalid_printer_id"))?;
    Ok(())
}

pub(super) fn validated_plate_id(value: i64) -> Result<u32, ApiError> {
    if value <= 0 {
        return Err(ApiError::bad_request("artifact_invalid_plate"));
    }
    u32::try_from(value).map_err(|_| ApiError::bad_request("artifact_invalid_plate"))
}

pub async fn list_jobs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> Result<Json<JobListResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant(&state, &headers, tenant_id, UserRole::Viewer).await?;
    let jobs = state
        .jobs()
        .list_for_tenant(tenant_id)
        .await?
        .into_iter()
        .map(JobResponse::try_from)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(JobListResponse { jobs }))
}

pub async fn get_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, job_id)): Path<(String, String)>,
) -> Result<Json<JobResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant(&state, &headers, tenant_id, UserRole::Viewer).await?;
    let job_id = JobId::parse(&job_id).map_err(|_| ApiError::bad_request("invalid_job_id"))?;
    let Some(job) = state.jobs().get_for_tenant(tenant_id, job_id).await? else {
        return Err(ApiError::not_found("job_not_found"));
    };

    Ok(Json(JobResponse::try_from(job)?))
}

impl TryFrom<JobWithArtifact> for JobResponse {
    type Error = RepositoryError;

    fn try_from(value: JobWithArtifact) -> Result<Self, Self::Error> {
        Self::from_parts(value.job, value.artifact)
    }
}

impl JobResponse {
    fn from_parts(job: Job, artifact: JobArtifact) -> Result<Self, RepositoryError> {
        let material = material::JobMaterialResponse::from_job(&job)?;
        Ok(Self {
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
            print: JobPrintResponse::from(job.print),
            command: JobCommandResponse {
                id: job.command_id.to_string(),
                kind: "print_project_file".to_string(),
                status: job.status.to_string(),
            },
            artifact: JobArtifactResponse::try_from_artifact(artifact)?,
            material,
        })
    }
}

impl From<JobPrintState> for JobPrintResponse {
    fn from(print: JobPrintState) -> Self {
        Self {
            status: print.status.to_string(),
            printer_state: print.printer_state,
            progress_percent: print.progress_percent,
            remaining_time_minutes: print.remaining_time_minutes,
            current_layer: print.current_layer,
            total_layers: print.total_layers,
            active_file: print.active_file,
            last_progress_percent: print.last_progress_percent,
            last_layer: print.last_layer,
            error: print.error,
            started_at: print.started_at,
            finished_at: print.finished_at,
            updated_at: print.updated_at,
        }
    }
}

impl JobArtifactResponse {
    fn try_from_artifact(artifact: JobArtifact) -> Result<Self, RepositoryError> {
        Ok(Self {
            id: artifact.id,
            tenant_id: artifact.tenant_id.to_string(),
            filename: artifact.filename,
            content_type: artifact.content_type,
            size_bytes: artifact.size_bytes,
            metadata: artifact
                .metadata_json
                .map(|value| serde_json::from_str(&value))
                .transpose()
                .map_err(|err| {
                    RepositoryError::Database(
                        anyhow::Error::new(err).context("invalid persisted artifact metadata"),
                    )
                })?,
            created_at: artifact.created_at,
        })
    }
}
