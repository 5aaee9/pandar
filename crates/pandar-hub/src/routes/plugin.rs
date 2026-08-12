use axum::{
    Json,
    extract::rejection::JsonRejection,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    AppState,
    repositories::{AuthenticatedPrincipal, NoAuthPluginSessionOutcome, RepositoryError},
    routes::{
        ApiError, auth,
        printer_operations::{PrinterOperationRequest, dispatch_plugin_printer_operation},
    },
};

mod camera;
pub(super) mod firmware;
mod h2c;
mod personal_presets;
mod responses;
mod studio_devices;
mod studio_jobs;
pub(super) use camera::stream_camera;
pub(super) use h2c::get_auto_nozzle_mapping;
pub(super) use personal_presets::{
    create as create_preset, delete as delete_preset, get as get_preset, list as list_presets,
    replace as replace_preset,
};
pub(crate) use responses::redact_artifact_error;
use studio_devices::{PluginPrinterListResponse, plugin_printer_devices};
pub(super) use studio_jobs::{
    cancel_job, create_print, get_job, get_model_task, get_plate, get_subtask, list_jobs,
};

#[derive(Debug, Deserialize)]
pub(super) struct CreateLoginTicketRequest {
    redirect_url: String,
    #[serde(default)]
    code_challenge: Option<String>,
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
    #[serde(default)]
    code_verifier: Option<String>,
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

pub(super) async fn create_mobile_login_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
    payload: Result<Json<CreateLoginTicketRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<LoginTicketResponse>), ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    let principal =
        auth::authorize_mobile_login_ticket_creation(&state, &headers, tenant_id).await?;
    let Json(payload) = payload.map_err(|_| ApiError::bad_request("bad_request"))?;
    let code_challenge = payload
        .code_challenge
        .filter(|value| valid_pkce_challenge(value))
        .ok_or_else(|| ApiError::bad_request("invalid_code_challenge"))?;
    let created = state
        .auth()
        .create_mobile_login_ticket_with_audit(
            tenant_id,
            user_id(&principal),
            payload.redirect_url,
            code_challenge,
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

pub(super) async fn exchange_mobile_login_ticket(
    State(state): State<AppState>,
    payload: Result<Json<ExchangeLoginTicketRequest>, JsonRejection>,
) -> Result<Json<ExchangeLoginTicketResponse>, ApiError> {
    let Json(payload) = payload.map_err(|_| ApiError::bad_request("bad_request"))?;
    let exchanged = state
        .auth()
        .exchange_mobile_login_ticket(
            &payload.ticket,
            payload.code_verifier.as_deref().unwrap_or_default(),
        )
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
        expires_at: token.expires_at.expect("mobile token must have expiry"),
        profile,
    }))
}

pub(super) async fn create_no_auth_session(
    State(state): State<AppState>,
) -> Result<Json<ExchangeLoginTicketResponse>, ApiError> {
    if !state.no_auth_enabled() {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "no_auth_required"));
    }
    let outcome = state
        .auth()
        .create_no_auth_plugin_session_with_audit(
            "Local Bambu Studio Plugin",
            plugin_session_expires_at()?,
        )
        .await?;
    let (tenant, token) = match outcome {
        NoAuthPluginSessionOutcome::Created(session) => {
            let session = *session;
            (session.tenant, session.tenant_token)
        }
        NoAuthPluginSessionOutcome::MissingTenant => {
            return Err(ApiError::not_found("tenant_not_found"));
        }
        NoAuthPluginSessionOutcome::AmbiguousTenant => {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "ambiguous_no_auth_tenant",
            ));
        }
    };
    let profile = PluginProfileResponse {
        user_id: token.token.id.clone(),
        user_name: token.token.name.clone(),
        tenant_id: token.token.tenant_id.to_string(),
        tenant_name: tenant.display_name.clone(),
    };

    Ok(Json(ExchangeLoginTicketResponse {
        token: token.plaintext_token,
        expires_at: token
            .token
            .expires_at
            .expect("plugin token must have expiry"),
        profile,
    }))
}

pub(super) async fn revoke_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let token = auth::bearer_token(&headers)?;
    state
        .auth()
        .revoke_plugin_studio_token_with_audit(token)
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "invalid_auth_token"))?;
    Ok(StatusCode::NO_CONTENT)
}

fn plugin_session_expires_at() -> Result<String, ApiError> {
    (OffsetDateTime::now_utc() + Duration::days(365))
        .format(&Rfc3339)
        .map_err(|err| {
            tracing::error!(error = %format!("{err:#}"), "failed to format plugin session expiry");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
        })
}

pub(super) async fn list_printers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<PluginPrinterListResponse>, ApiError> {
    let authenticated = auth::authorize_plugin_studio(&state, &headers).await?;
    Ok(Json(PluginPrinterListResponse {
        message: "success",
        devices: plugin_printer_devices(&state, authenticated.token.tenant_id).await?,
    }))
}

pub(super) async fn create_printer_operation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(printer_id): Path<String>,
    payload: Result<Json<PrinterOperationRequest>, JsonRejection>,
) -> Result<Json<PluginPrinterOperationResponse>, ApiError> {
    let authenticated = auth::authorize_plugin_studio(&state, &headers).await?;
    let Json(payload) = payload.map_err(|_| ApiError::bad_request("invalid_printer_control"))?;
    let command = dispatch_plugin_printer_operation(
        &state,
        authenticated.token.tenant_id,
        &printer_id,
        payload,
        auth::plugin_audit_actor(&authenticated),
    )
    .await?;

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
        AuthenticatedPrincipal::NoAuth { .. } => None,
    }
}

fn valid_pkce_challenge(value: &str) -> bool {
    value.len() == 43
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn plugin_ticket_error(err: RepositoryError) -> ApiError {
    match err {
        RepositoryError::MissingPluginLoginTicket => {
            ApiError::new(StatusCode::UNAUTHORIZED, "invalid_plugin_ticket")
        }
        other => other.into(),
    }
}

fn plugin_login_ticket_expires_at() -> Result<String, ApiError> {
    (OffsetDateTime::now_utc() + Duration::minutes(5))
        .format(&Rfc3339)
        .map_err(|err| {
            tracing::error!(error = %format!("{err:#}"), "failed to format plugin login ticket expiry");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
        })
}
