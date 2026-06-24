use axum::{
    Json,
    extract::rejection::JsonRejection,
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    AppState,
    repositories::{AuthenticatedPrincipal, JobWithArtifact, RepositoryError},
    routes::{ApiError, auth, printer_operations::PrinterOperationRequest},
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
    artifact_metadata: Option<serde_json::Value>,
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
    artifact_metadata: Option<serde_json::Value>,
    pandar_job_id: String,
}

#[derive(Debug, Serialize)]
pub(super) struct PluginPrinterOperationResponse {
    command_id: String,
    status: String,
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
        .map(PluginJobResponse::try_from)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(PluginJobListResponse { jobs }))
}

pub(super) async fn create_print(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<(StatusCode, Json<PluginPrintResponse>), ApiError> {
    let authenticated = auth::authorize_plugin_studio(&state, &headers).await?;
    let tenant_id = authenticated.token.tenant_id;
    let created = super::jobs::multipart::create_print_job_from_multipart(
        &state,
        tenant_id,
        None,
        multipart,
        auth::plugin_audit_actor(&authenticated),
        "plugin",
    )
    .await?;
    let wake_tenant_id = created.job.tenant_id;
    let wake_agent_id = created.job.agent_id;
    let response = PluginPrintResponse::try_from(created)?;
    state.wake_agent(wake_tenant_id, wake_agent_id).await;
    Ok((StatusCode::CREATED, Json(response)))
}

pub(super) async fn create_printer_operation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(printer_id): Path<String>,
    payload: Result<Json<PrinterOperationRequest>, JsonRejection>,
) -> Result<Json<PluginPrinterOperationResponse>, ApiError> {
    let authenticated = auth::authorize_plugin_studio(&state, &headers).await?;
    let Json(payload) = payload.map_err(|_| ApiError::bad_request("invalid_printer_control"))?;
    let operation = payload.into_operation()?;
    let command = state
        .commands()
        .enqueue_printer_operation_with_audit(
            authenticated.token.tenant_id,
            &printer_id,
            operation,
            auth::plugin_audit_actor(&authenticated),
        )
        .await
        .map_err(plugin_operation_error)?;
    state.wake_agent(command.tenant_id, command.agent_id).await;

    Ok(Json(PluginPrinterOperationResponse {
        command_id: command.id.to_string(),
        status: command.status.to_string(),
    }))
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

fn plugin_operation_error(err: RepositoryError) -> ApiError {
    match err {
        RepositoryError::PrinterControlUnavailable => {
            ApiError::bad_request("printer_operation_unavailable")
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

impl TryFrom<JobWithArtifact> for PluginJobResponse {
    type Error = RepositoryError;

    fn try_from(value: JobWithArtifact) -> Result<Self, Self::Error> {
        Ok(Self {
            task_id: value.job.id.to_string(),
            dev_id: value.job.printer_id,
            name: value.artifact.filename,
            status: value.job.status.to_string(),
            progress_percent: value.job.print.progress_percent,
            artifact_metadata: artifact_metadata(value.artifact.metadata_json)?,
            created_at: value.job.created_at,
            updated_at: value.job.updated_at,
            pandar_job_id: value.job.id.to_string(),
        })
    }
}

impl TryFrom<JobWithArtifact> for PluginPrintResponse {
    type Error = RepositoryError;

    fn try_from(value: JobWithArtifact) -> Result<Self, Self::Error> {
        Ok(Self {
            task_id: value.job.id.to_string(),
            command_id: value.job.command_id.to_string(),
            status: value.job.status.to_string(),
            message: None,
            artifact_metadata: artifact_metadata(value.artifact.metadata_json)?,
            pandar_job_id: value.job.id.to_string(),
        })
    }
}

fn artifact_metadata(
    metadata_json: Option<String>,
) -> Result<Option<serde_json::Value>, RepositoryError> {
    metadata_json
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(|err| {
            RepositoryError::Database(
                anyhow::Error::new(err).context("invalid persisted artifact metadata"),
            )
        })
}
