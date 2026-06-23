use axum::{
    Json,
    extract::State,
    extract::rejection::JsonRejection,
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    AppState,
    repositories::{AuthenticatedPrincipal, CreatePrintJob, JobWithArtifact, RepositoryError},
    routes::{ApiError, auth, jobs::validate_artifact_submission},
};

#[derive(Debug, Deserialize)]
pub(super) struct CreateLoginTicketRequest {
    redirect_url: String,
}

#[derive(Debug, Serialize)]
pub(super) struct LoginTicketResponse {
    ticket: String,
    expires_at: String,
    redirect_url: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ExchangeLoginTicketRequest {
    ticket: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ExchangeLoginTicketResponse {
    token: String,
    expires_at: String,
    profile: PluginProfileResponse,
}

#[derive(Debug, Serialize)]
pub(super) struct PluginProfileResponse {
    user_id: String,
    user_name: String,
    tenant_id: String,
    tenant_name: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreatePluginPrintRequest {
    printer_id: String,
    filename: String,
    content_type: String,
    artifact_base64: String,
    plate_id: i64,
    use_ams: bool,
    flow_cali: bool,
    timelapse: bool,
    ams_mapping: Option<Value>,
    ams_mapping2: Option<Value>,
}

#[derive(Debug, Serialize)]
pub(super) struct PluginPrinterListResponse {
    printers: Vec<PluginPrinterResponse>,
}

#[derive(Debug, Serialize)]
pub(super) struct PluginPrinterResponse {
    dev_id: String,
    name: String,
    model: Option<String>,
    online: bool,
    state: String,
    pandar_printer_id: String,
}

#[derive(Debug, Serialize)]
pub(super) struct PluginJobListResponse {
    jobs: Vec<PluginJobResponse>,
}

#[derive(Debug, Serialize)]
pub(super) struct PluginJobResponse {
    task_id: String,
    dev_id: String,
    name: String,
    status: String,
    progress_percent: Option<u8>,
    created_at: String,
    updated_at: String,
    pandar_job_id: String,
}

#[derive(Debug, Serialize)]
pub(super) struct PluginPrintResponse {
    task_id: String,
    command_id: String,
    status: String,
    message: Option<String>,
    pandar_job_id: String,
}

pub(super) async fn create_login_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
    payload: Result<Json<CreateLoginTicketRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<LoginTicketResponse>), ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    let principal =
        auth::authorize_plugin_login_ticket_creation(&state, &headers, tenant_id).await?;
    let Json(payload) = payload.map_err(|_| ApiError::bad_request("bad_request"))?;
    let created = state
        .auth()
        .create_plugin_login_ticket_with_audit(
            tenant_id,
            user_id(&principal),
            payload.redirect_url,
            plugin_login_ticket_expires_at()?,
            auth::audit_actor(&principal),
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(LoginTicketResponse {
            ticket: created.plaintext_ticket,
            expires_at: created.ticket.expires_at,
            redirect_url: created.ticket.redirect_url,
        }),
    ))
}

pub(super) async fn exchange_login_ticket(
    State(state): State<AppState>,
    payload: Result<Json<ExchangeLoginTicketRequest>, JsonRejection>,
) -> Result<Json<ExchangeLoginTicketResponse>, ApiError> {
    let Json(payload) = payload.map_err(|_| ApiError::bad_request("bad_request"))?;
    let exchanged = state
        .auth()
        .exchange_plugin_login_ticket(&payload.ticket)
        .await
        .map_err(plugin_ticket_error)?
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "invalid_plugin_ticket"))?;
    let token = exchanged.tenant_token.token;
    let tenant = state
        .tenants()
        .get(token.tenant_id)
        .await?
        .ok_or_else(|| ApiError::not_found("tenant_not_found"))?;
    let profile = PluginProfileResponse {
        user_id: token
            .created_by_user_id
            .clone()
            .unwrap_or_else(|| token.id.clone()),
        user_name: token.name.clone(),
        tenant_id: token.tenant_id.to_string(),
        tenant_name: tenant.display_name,
    };

    Ok(Json(ExchangeLoginTicketResponse {
        token: exchanged.tenant_token.plaintext_token,
        expires_at: token.expires_at.expect("plugin token must have expiry"),
        profile,
    }))
}

pub(super) async fn list_printers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<PluginPrinterListResponse>, ApiError> {
    let authenticated = auth::authorize_plugin_studio(&state, &headers).await?;
    let tenant_id = authenticated.token.tenant_id;
    let printers = state
        .printers()
        .list_for_tenant(tenant_id)
        .await?
        .into_iter()
        .map(|printer| PluginPrinterResponse {
            dev_id: printer.id.clone(),
            name: printer.name,
            model: printer.model,
            online: printer.status == "online",
            state: printer.status,
            pandar_printer_id: printer.id,
        })
        .collect();

    Ok(Json(PluginPrinterListResponse { printers }))
}

pub(super) async fn list_jobs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<PluginJobListResponse>, ApiError> {
    let authenticated = auth::authorize_plugin_studio(&state, &headers).await?;
    let jobs = state
        .jobs()
        .list_for_tenant(authenticated.token.tenant_id)
        .await?
        .into_iter()
        .map(PluginJobResponse::from)
        .collect();

    Ok(Json(PluginJobListResponse { jobs }))
}

pub(super) async fn create_print(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<CreatePluginPrintRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<PluginPrintResponse>), ApiError> {
    let authenticated = auth::authorize_plugin_studio(&state, &headers).await?;
    let tenant_id = authenticated.token.tenant_id;
    let Json(payload) = payload.map_err(|_| ApiError::bad_request("bad_request"))?;
    if payload.filename.trim().is_empty() {
        return Err(ApiError::bad_request("bad_request"));
    }
    if payload.artifact_base64.trim().is_empty() {
        return Err(ApiError::bad_request("artifact_empty"));
    }
    let plate_id = super::jobs::validated_plate_id(payload.plate_id)?;
    uuid::Uuid::parse_str(&payload.printer_id)
        .map_err(|_| ApiError::bad_request("invalid_printer_id"))?;

    let content_type = if payload.content_type.trim().is_empty() {
        "application/octet-stream".to_string()
    } else {
        payload.content_type
    };
    let artifact_bytes = validate_artifact_submission(
        &payload.artifact_base64,
        state.job_storage().max_artifact_bytes(),
    )?;

    let ams_mapping_json = mapping_json(payload.ams_mapping, "ams_mapping")?;
    let ams_mapping2_json = mapping_json(payload.ams_mapping2, "ams_mapping2")?;
    let Some(printer) = state
        .printers()
        .get_for_tenant(tenant_id, &payload.printer_id)
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
            tracing::error!(
                error = %redact_artifact_error(&format!("{err:#}")),
                "failed to write plugin print artifact"
            );
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
        })?;
    let created = state
        .jobs()
        .create_print_job_with_audit(
            CreatePrintJob {
                tenant_id,
                printer_id: printer.id,
                agent_id: printer.agent_id,
                artifact_id,
                artifact_filename: stored.filename,
                artifact_content_type: content_type,
                artifact_size_bytes: stored.size_bytes,
                artifact_storage_path: stored.storage_path.clone(),
                plate_id,
                use_ams: payload.use_ams,
                flow_cali: payload.flow_cali,
                timelapse: payload.timelapse,
                ams_mapping_json,
                ams_mapping2_json,
            },
            auth::plugin_audit_actor(&authenticated),
        )
        .await;

    match created {
        Ok(created) => Ok((
            StatusCode::CREATED,
            Json(PluginPrintResponse::from(created)),
        )),
        Err(err) => {
            if let Err(cleanup_err) = state
                .job_storage()
                .remove_artifact(&stored.storage_path)
                .await
            {
                tracing::warn!(
                    error = %redact_artifact_error(&format!("{cleanup_err:#}")),
                    "failed to remove plugin print artifact after repository error"
                );
            }
            Err(err.into())
        }
    }
}

fn user_id(principal: &AuthenticatedPrincipal) -> Option<String> {
    match principal {
        AuthenticatedPrincipal::User(authenticated) => Some(authenticated.user.id.clone()),
        AuthenticatedPrincipal::TenantToken(authenticated) => {
            authenticated.token.created_by_user_id.clone()
        }
    }
}

fn plugin_ticket_error(err: RepositoryError) -> ApiError {
    match err {
        RepositoryError::MissingPluginLoginTicket => {
            ApiError::new(StatusCode::UNAUTHORIZED, "invalid_plugin_ticket")
        }
        other => other.into(),
    }
}

pub(super) fn redact_artifact_error(message: &str) -> String {
    message
        .lines()
        .map(|line| {
            if line.contains("artifact directory ")
                || line.contains("artifact file ")
                || line.contains("artifact storage path ")
            {
                line.split_once("artifact")
                    .map(|(prefix, suffix)| {
                        format!("{prefix}artifact{}", redact_artifact_path(suffix))
                    })
                    .unwrap_or_else(|| line.to_owned())
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_artifact_path(suffix: &str) -> String {
    for marker in [" directory ", " file ", " storage path "] {
        if let Some((prefix, _)) = suffix.split_once(marker) {
            return format!("{prefix}{marker}[redacted]");
        }
    }
    suffix.to_owned()
}

fn plugin_login_ticket_expires_at() -> Result<String, ApiError> {
    (OffsetDateTime::now_utc() + Duration::minutes(5))
        .format(&Rfc3339)
        .map_err(|err| {
            tracing::error!(error = %format!("{err:#}"), "failed to format plugin login ticket expiry");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
        })
}

fn mapping_json(value: Option<Value>, field: &'static str) -> Result<Option<String>, ApiError> {
    value
        .map(|value| {
            serde_json::to_string(&value).map_err(|err| {
                tracing::error!(
                    error = %format!("{err:#}"),
                    field,
                    "failed to serialize plugin print material mapping"
                );
                ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
            })
        })
        .transpose()
}

impl From<JobWithArtifact> for PluginJobResponse {
    fn from(value: JobWithArtifact) -> Self {
        Self {
            task_id: value.job.id.to_string(),
            dev_id: value.job.printer_id,
            name: value.artifact.filename,
            status: value.job.status.to_string(),
            progress_percent: value.job.print.progress_percent,
            created_at: value.job.created_at,
            updated_at: value.job.updated_at,
            pandar_job_id: value.job.id.to_string(),
        }
    }
}

impl From<JobWithArtifact> for PluginPrintResponse {
    fn from(value: JobWithArtifact) -> Self {
        Self {
            task_id: value.job.id.to_string(),
            command_id: value.job.command_id.to_string(),
            status: value.job.status.to_string(),
            message: None,
            pandar_job_id: value.job.id.to_string(),
        }
    }
}
