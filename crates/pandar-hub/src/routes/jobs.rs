use crate::artifacts::metadata::ArtifactMetadata;
use axum::{
    Json,
    extract::Multipart,
    extract::rejection::JsonRejection,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use pandar_core::JobId;
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    repositories::UserRole,
    routes::{ApiError, auth, parse_tenant_id},
};

mod delete;
mod material;
mod metadata_preview;
pub(super) mod multipart;
mod recovery_request;
pub(super) use delete::delete_job;

use recovery_request::{DuplicateJobRequest, ReprintJobRequest};

pub use crate::job_projection::JobProjection as JobResponse;

#[derive(Debug, Deserialize)]
pub struct RecoveryReasonRequest {
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ArtifactMetadataPreviewResponse {
    metadata: Option<ArtifactMetadata>,
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
        multipart::MultipartPrintKind::Web,
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
        metadata_preview::preview_artifact_metadata_from_multipart(&state, tenant_id, multipart)
            .await?;
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
    payload: Result<Json<ReprintJobRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<JobResponse>), ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let auth =
        auth::authorize_tenant_principal(&state, &headers, tenant_id, UserRole::Operator).await?;
    let job_id = JobId::parse(&job_id).map_err(|_| ApiError::bad_request("invalid_job_id"))?;
    let Json(payload) = payload.map_err(|_| ApiError::bad_request("bad_request"))?;
    let reason = payload.reason;
    let overrides = payload.overrides.into_repository()?;
    let job = state
        .jobs()
        .reprint_with_audit(
            tenant_id,
            job_id,
            overrides,
            reason,
            auth::audit_actor(&auth),
        )
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
    let job = state
        .jobs()
        .duplicate_and_print_with_audit(
            tenant_id,
            job_id,
            payload.into_repository()?,
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
    if !(1..=i64::from(i32::MAX)).contains(&value) {
        return Err(ApiError::bad_request("artifact_invalid_plate"));
    }
    Ok(value as u32)
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

pub async fn clear_jobs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> Result<Json<crate::repositories::ClearJobsOutcome>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let auth = auth::authorize_tenant_admin_principal(&state, &headers, tenant_id).await?;
    let outcome = state
        .jobs()
        .clear_for_tenant_with_audit(
            state.artifact_storage(),
            tenant_id,
            auth::audit_actor(&auth),
        )
        .await?;
    Ok(Json(outcome))
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
