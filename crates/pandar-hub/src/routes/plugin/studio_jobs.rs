use std::collections::HashMap;

use axum::{
    Json,
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use pandar_core::{Job, JobStatus, PrintTransferFailure, StudioSubmissionId, TenantId};
use serde::Deserialize;

use crate::{
    AppState,
    repositories::{RepositoryError, StudioTaskQuery, StudioTaskStatus},
    routes::{ApiError, auth},
};

mod model_task;
mod responses;
mod subtask;
use responses::{
    StudioCreatePrintResponse, StudioTaskDetailResponse, StudioTaskPageResponse, task_hit_from_job,
};

#[derive(Debug, Deserialize)]
pub(crate) struct StudioTaskQueryParams {
    dev_id: Option<String>,
    status: Option<i32>,
    offset: Option<i64>,
    limit: Option<i64>,
}

pub(crate) async fn create_print(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<(StatusCode, Json<StudioCreatePrintResponse>), ApiError> {
    let authenticated = auth::authorize_plugin_studio(&state, &headers).await?;
    let tenant_id = authenticated.token.tenant_id;
    let created = super::super::jobs::multipart::create_print_job_from_multipart(
        &state,
        tenant_id,
        None,
        multipart,
        auth::plugin_audit_actor(&authenticated),
        "plugin",
        super::super::jobs::multipart::MultipartPrintKind::Studio,
    )
    .await?;
    let wake_tenant_id = created.job.tenant_id;
    let wake_agent_id = created.job.agent_id;
    let response = StudioCreatePrintResponse::from(&created);
    state.wake_agent(wake_tenant_id, wake_agent_id).await;
    Ok((StatusCode::CREATED, Json(response)))
}

pub(crate) async fn list_jobs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<StudioTaskQueryParams>,
) -> Result<Json<StudioTaskPageResponse>, ApiError> {
    let authenticated = auth::authorize_plugin_studio(&state, &headers).await?;
    let tenant_id = authenticated.token.tenant_id;
    let query = repository_query(&state, tenant_id, params).await?;
    let page = state.jobs().list_studio_tasks(tenant_id, query).await?;
    let printers = state
        .printers()
        .list_for_tenant(tenant_id)
        .await?
        .into_iter()
        .map(|printer| (printer.id.clone(), printer))
        .collect::<HashMap<_, _>>();
    let hits = page
        .jobs
        .into_iter()
        .map(|job| {
            let printer = printers
                .get(&job.job.printer_id)
                .ok_or_else(|| ApiError::not_found("printer_not_found"))?;
            Ok::<_, ApiError>(task_hit_from_job(job, printer))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(StudioTaskPageResponse {
        total: page.total,
        hits,
    }))
}

pub(crate) async fn get_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<StudioTaskDetailResponse>, ApiError> {
    let (tenant_id, id) = authorized_id(&state, &headers, &id).await?;
    let job = load_job(&state, tenant_id, id).await?;
    let failure = load_print_transfer_failure(&state, tenant_id, &job.job).await?;
    Ok(Json(StudioTaskDetailResponse::from_job(&job.job, failure)))
}

pub(crate) async fn get_plate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<responses::StudioPlateResponse>, ApiError> {
    let (tenant_id, id) = authorized_id(&state, &headers, &id).await?;
    let job = load_job(&state, tenant_id, id).await?;
    Ok(Json(responses::StudioPlateResponse::from(&job.job)))
}

pub(crate) async fn get_model_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<model_task::StudioModelTaskResponse>, ApiError> {
    let raw_id = id;
    let (tenant_id, id) = authorized_id(&state, &headers, &raw_id).await?;
    if raw_id != id.get().to_string() {
        return Err(ApiError::bad_request("invalid_studio_submission_id"));
    }
    let job = state
        .jobs()
        .get_by_studio_submission_id(tenant_id, id)
        .await
        .map_err(|err| match err {
            RepositoryError::InvalidPersistedStudioMetadata(err) => {
                tracing::error!(error = %format!("{err:#}"), "invalid persisted Studio model-task metadata");
                model_task::unavailable()
            }
            other => other.into(),
        })?
        .ok_or_else(|| ApiError::not_found("job_not_found"))?;
    Ok(Json(model_task::from_job(job)?))
}

pub(crate) async fn get_subtask(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<subtask::StudioSubtaskResponse>, ApiError> {
    let (tenant_id, id) = authorized_id(&state, &headers, &id).await?;
    let job = state
        .jobs()
        .get_by_studio_submission_id(tenant_id, id)
        .await
        .map_err(|err| match err {
            RepositoryError::InvalidPersistedArtifactMetadata(err) => {
                tracing::error!(error = %format!("{err:#}"), "invalid persisted Studio artifact metadata");
                ApiError::new(
                    StatusCode::CONFLICT,
                    "studio_task_metadata_unavailable",
                )
            }
            other => other.into(),
        })?
        .ok_or_else(|| ApiError::not_found("job_not_found"))?;
    Ok(Json(subtask::from_job(job)?))
}

pub(crate) async fn cancel_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<StudioTaskDetailResponse>, ApiError> {
    let authenticated = auth::authorize_plugin_studio(&state, &headers).await?;
    let tenant_id = authenticated.token.tenant_id;
    let id = parse_id(&id)?;
    let job = state
        .jobs()
        .cancel_studio_print_with_audit(tenant_id, id, auth::plugin_audit_actor(&authenticated))
        .await
        .map_err(|err| match err {
            RepositoryError::StudioCancellationTooLate => {
                ApiError::new(StatusCode::CONFLICT, "cancel_too_late")
            }
            other => other.into(),
        })?;
    Ok(Json(StudioTaskDetailResponse::from_job(&job.job, None)))
}

async fn repository_query(
    state: &AppState,
    tenant_id: TenantId,
    params: StudioTaskQueryParams,
) -> Result<StudioTaskQuery, ApiError> {
    let status = match params.status.unwrap_or(0) {
        0 => None,
        1 => Some(StudioTaskStatus::InProgress),
        2 => Some(StudioTaskStatus::Completed),
        3 => Some(StudioTaskStatus::Failed),
        _ => return Err(ApiError::bad_request("invalid_task_status")),
    };
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(20);
    if offset < 0 || !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("invalid_task_pagination"));
    }
    let printer_id = match params.dev_id.filter(|value| !value.is_empty()) {
        Some(serial) => {
            let Some(printer) = state
                .printers()
                .get_by_serial_for_tenant(tenant_id, &serial)
                .await?
            else {
                return Err(ApiError::not_found("printer_not_found"));
            };
            Some(printer.id)
        }
        None => None,
    };
    Ok(StudioTaskQuery {
        printer_id,
        status,
        offset: offset as u64,
        limit: limit as u64,
    })
}

async fn authorized_id(
    state: &AppState,
    headers: &HeaderMap,
    raw_id: &str,
) -> Result<(TenantId, StudioSubmissionId), ApiError> {
    let authenticated = auth::authorize_plugin_studio(state, headers).await?;
    Ok((authenticated.token.tenant_id, parse_id(raw_id)?))
}

fn parse_id(value: &str) -> Result<StudioSubmissionId, ApiError> {
    let value = value
        .parse::<i64>()
        .map_err(|_| ApiError::bad_request("invalid_studio_submission_id"))?;
    StudioSubmissionId::try_from(value)
        .map_err(|_| ApiError::bad_request("invalid_studio_submission_id"))
}

async fn load_print_transfer_failure(
    state: &AppState,
    tenant_id: TenantId,
    job: &Job,
) -> Result<Option<PrintTransferFailure>, ApiError> {
    if job.status != JobStatus::Failed {
        return Ok(None);
    }
    let command = state
        .commands()
        .get_for_tenant(tenant_id, job.command_id)
        .await?
        .ok_or(RepositoryError::MissingCommand)?;
    let Some(result_json) = command.result_json else {
        return Ok(None);
    };
    let mut failure =
        serde_json::from_str::<PrintTransferFailure>(&result_json).map_err(|err| {
            RepositoryError::Database(
                anyhow::Error::new(err).context("invalid persisted print transfer failure"),
            )
        })?;
    failure.cause = job.error.clone().ok_or_else(|| {
        RepositoryError::Database(anyhow::anyhow!(
            "failed print transfer is missing its persisted cause"
        ))
    })?;
    Ok(Some(failure))
}

async fn load_job(
    state: &AppState,
    tenant_id: TenantId,
    id: StudioSubmissionId,
) -> Result<crate::repositories::JobWithArtifact, ApiError> {
    state
        .jobs()
        .get_by_studio_submission_id(tenant_id, id)
        .await?
        .ok_or_else(|| ApiError::not_found("job_not_found"))
}
