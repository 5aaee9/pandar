use axum::{
    Json,
    extract::rejection::JsonRejection,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use pandar_core::{
    AgentId, FirmwareCatalogEntry, FirmwareCommand, FirmwareControlMetadata, PrinterFirmwareState,
    TenantId,
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    firmware_control::{
        FirmwareExecutePhase, FirmwareExecuteResult, FirmwareRefreshResult, FirmwareServiceError,
        PreparedFirmwareControl,
    },
    repositories::RepositoryError,
    routes::{ApiError, auth},
};
use pandar_protocol::agent::v1::AgentCapability;

mod ownership;

pub(crate) const FIRMWARE_EXECUTE_BODY_LIMIT: usize = 64 * 1024 - r#"{"upgrade":}"#.len()
    + r#"{"prepared_token":"00000000-0000-0000-0000-000000000000","command":}"#.len();

#[derive(Debug, Serialize)]
pub(crate) struct FirmwareStateResponse {
    firmware: PrinterFirmwareState,
    catalog: Vec<FirmwareCatalogEntry>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FirmwareRefreshRequest {
    sequence_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FirmwareExecuteRequest {
    prepared_token: String,
    command: FirmwareCommand,
}

#[derive(Debug, Serialize)]
struct FirmwareErrorBody {
    error: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    phase: Option<FirmwareExecutePhase>,
}

#[derive(Debug)]
pub(crate) struct FirmwareApiError {
    status: StatusCode,
    error: &'static str,
    phase: Option<FirmwareExecutePhase>,
}

pub(crate) async fn get_state(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(printer_id): Path<String>,
) -> Result<Json<FirmwareStateResponse>, FirmwareApiError> {
    let authenticated = auth::authorize_plugin_studio(&state, &headers).await?;
    let tenant_id = authenticated.token.tenant_id;
    let printer = state
        .printers()
        .get_with_live_status_for_tenant(tenant_id, &printer_id)
        .await?
        .ok_or_else(FirmwareApiError::printer_not_found)?;
    let firmware = current_firmware_projection(
        &state,
        tenant_id,
        printer.printer.agent_id,
        printer.firmware,
    )
    .await?
    .ok_or_else(FirmwareApiError::unavailable)?;
    Ok(Json(FirmwareStateResponse {
        firmware,
        catalog: Vec::new(),
    }))
}

pub(crate) async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(printer_id): Path<String>,
    payload: Result<Json<FirmwareRefreshRequest>, JsonRejection>,
) -> Result<Json<FirmwareRefreshResult>, FirmwareApiError> {
    let authenticated = auth::authorize_plugin_studio(&state, &headers).await?;
    require_printer(&state, authenticated.token.tenant_id, &printer_id).await?;
    let Json(payload) = payload.map_err(|_| FirmwareApiError::invalid_request())?;
    require_non_empty(&payload.sequence_id)?;
    let result = state
        .refresh_version(
            authenticated.token.tenant_id,
            &printer_id,
            payload.sequence_id,
            auth::plugin_audit_actor(&authenticated),
        )
        .await
        .map_err(FirmwareApiError::refresh_service)?;
    Ok(Json(result))
}

pub(crate) async fn prepare(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(printer_id): Path<String>,
    payload: Result<Json<FirmwareControlMetadata>, JsonRejection>,
) -> Result<Json<PreparedFirmwareControl>, FirmwareApiError> {
    let authenticated = auth::authorize_plugin_studio(&state, &headers).await?;
    require_printer(&state, authenticated.token.tenant_id, &printer_id).await?;
    let Json(payload) = payload.map_err(|_| FirmwareApiError::invalid_request())?;
    validate_metadata(&payload)?;
    let result = state
        .prepare_control(
            authenticated.token.tenant_id,
            &printer_id,
            payload,
            auth::plugin_audit_actor(&authenticated),
        )
        .await
        .map_err(FirmwareApiError::prepare_service)?;
    Ok(Json(result))
}

pub(crate) async fn execute(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(printer_id): Path<String>,
    payload: Result<Json<FirmwareExecuteRequest>, JsonRejection>,
) -> Result<Json<FirmwareExecuteResult>, FirmwareApiError> {
    let authenticated = auth::authorize_plugin_studio(&state, &headers)
        .await
        .map_err(|error| FirmwareApiError::from(error).with_pre_publish_phase())?;
    let tenant_id = authenticated.token.tenant_id;
    require_printer(&state, tenant_id, &printer_id)
        .await
        .map_err(FirmwareApiError::with_pre_publish_phase)?;
    let Json(payload) =
        payload.map_err(|_| FirmwareApiError::invalid_request().with_pre_publish_phase())?;
    require_non_empty(&payload.prepared_token).map_err(FirmwareApiError::with_pre_publish_phase)?;
    validate_command(&payload.command).map_err(FirmwareApiError::with_pre_publish_phase)?;
    ownership::require_prepared_token_for_path(
        &state,
        tenant_id,
        &printer_id,
        &payload.prepared_token,
    )
    .await
    .map_err(FirmwareApiError::with_pre_publish_phase)?;
    let result = state
        .execute_control(tenant_id, &payload.prepared_token, payload.command)
        .await
        .map_err(FirmwareApiError::execute_service)?;
    Ok(Json(result))
}

pub(super) async fn current_firmware_projection(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    firmware: PrinterFirmwareState,
) -> Result<Option<PrinterFirmwareState>, ApiError> {
    let Some(token) = state
        .sessions()
        .current_token_for_capability(tenant_id, agent_id, AgentCapability::FirmwareControl)
        .await
    else {
        return Ok(None);
    };
    if firmware.session_id.as_deref() != Some(token.persisted_id().as_str())
        || firmware.generation.is_none()
    {
        return Ok(None);
    }
    let fence = match state
        .agents()
        .begin_current_session_fence(tenant_id, agent_id, &token.persisted_id())
        .await
    {
        Ok(fence) => fence,
        Err(RepositoryError::AgentSessionNotCurrent | RepositoryError::MissingAgent) => {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    fence.commit().await.map_err(|error| {
        tracing::error!(
            error = %format!("{:#}", anyhow::Error::new(error).context("failed to release firmware projection session fence")),
            "failed to validate current firmware projection"
        );
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
    })?;
    Ok(Some(firmware))
}

async fn require_printer(
    state: &AppState,
    tenant_id: TenantId,
    printer_id: &str,
) -> Result<(), FirmwareApiError> {
    state
        .printers()
        .get_for_tenant(tenant_id, printer_id)
        .await?
        .ok_or_else(FirmwareApiError::printer_not_found)?;
    Ok(())
}

fn validate_metadata(metadata: &FirmwareControlMetadata) -> Result<(), FirmwareApiError> {
    match metadata {
        FirmwareControlMetadata::UpgradeConfirm { sequence_id, .. }
        | FirmwareControlMetadata::ConsistencyConfirm { sequence_id, .. }
        | FirmwareControlMetadata::SwitchAmsFirmware { sequence_id, .. } => {
            require_non_empty(sequence_id)
        }
        FirmwareControlMetadata::Start {
            sequence_id,
            module,
            version,
            ..
        } => {
            require_non_empty(sequence_id)?;
            require_non_empty(module)?;
            require_non_empty(version)
        }
    }
}

fn validate_command(command: &FirmwareCommand) -> Result<(), FirmwareApiError> {
    validate_metadata(&FirmwareControlMetadata::from(command))?;
    if let FirmwareCommand::Start { url, .. } = command {
        require_non_empty(url)?;
    }
    Ok(())
}

fn require_non_empty(value: &str) -> Result<(), FirmwareApiError> {
    if value.trim().is_empty() {
        Err(FirmwareApiError::invalid_request())
    } else {
        Ok(())
    }
}

impl FirmwareApiError {
    fn new(status: StatusCode, error: &'static str, phase: Option<FirmwareExecutePhase>) -> Self {
        Self {
            status,
            error,
            phase,
        }
    }

    fn invalid_request() -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_firmware_request", None)
    }

    fn printer_not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "printer_not_found", None)
    }

    fn unavailable() -> Self {
        Self::new(StatusCode::CONFLICT, "firmware_control_unavailable", None)
    }

    fn invalid_prepared_token() -> Self {
        Self::new(
            StatusCode::CONFLICT,
            "invalid_firmware_prepared_token",
            Some(FirmwareExecutePhase::PrePublishFailure),
        )
    }

    fn with_pre_publish_phase(mut self) -> Self {
        self.phase = Some(FirmwareExecutePhase::PrePublishFailure);
        self
    }

    fn prepare_service(error: FirmwareServiceError) -> Self {
        match error {
            FirmwareServiceError::Unavailable => Self::new(
                StatusCode::CONFLICT,
                "firmware_control_unavailable",
                Some(FirmwareExecutePhase::PrePublishFailure),
            ),
            FirmwareServiceError::CommandFailed { .. } => Self::new(
                StatusCode::CONFLICT,
                "firmware_pre_publish_failure",
                Some(FirmwareExecutePhase::PrePublishFailure),
            ),
            FirmwareServiceError::InvalidPreparedToken | FirmwareServiceError::MetadataMismatch => {
                Self::invalid_request()
            }
            FirmwareServiceError::Internal { source }
            | FirmwareServiceError::InternalPrePublish { source } => {
                log_service_error("firmware prepare failed", source);
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_server_error",
                    Some(FirmwareExecutePhase::PrePublishFailure),
                )
            }
        }
    }

    fn refresh_service(error: FirmwareServiceError) -> Self {
        match error {
            FirmwareServiceError::Unavailable => Self::unavailable(),
            FirmwareServiceError::CommandFailed { .. }
            | FirmwareServiceError::InvalidPreparedToken
            | FirmwareServiceError::MetadataMismatch => {
                Self::new(StatusCode::BAD_GATEWAY, "firmware_refresh_failed", None)
            }
            FirmwareServiceError::Internal { source }
            | FirmwareServiceError::InternalPrePublish { source } => {
                log_service_error("firmware refresh failed", source);
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "firmware_refresh_failed",
                    None,
                )
            }
        }
    }

    fn execute_service(error: FirmwareServiceError) -> Self {
        match error {
            FirmwareServiceError::InvalidPreparedToken => Self::invalid_prepared_token(),
            FirmwareServiceError::MetadataMismatch => Self::new(
                StatusCode::CONFLICT,
                "firmware_metadata_mismatch",
                Some(FirmwareExecutePhase::PrePublishFailure),
            ),
            FirmwareServiceError::Unavailable => Self::new(
                StatusCode::CONFLICT,
                "firmware_control_unavailable",
                Some(FirmwareExecutePhase::PrePublishFailure),
            ),
            FirmwareServiceError::CommandFailed { .. } => Self::new(
                StatusCode::CONFLICT,
                "firmware_pre_publish_failure",
                Some(FirmwareExecutePhase::PrePublishFailure),
            ),
            FirmwareServiceError::InternalPrePublish { source } => {
                log_service_error("firmware execute failed before dispatch", source);
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_server_error",
                    Some(FirmwareExecutePhase::PrePublishFailure),
                )
            }
            FirmwareServiceError::Internal { source } => {
                log_service_error("firmware execute failed with ambiguous outcome", source);
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "firmware_outcome_unknown",
                    Some(FirmwareExecutePhase::OutcomeUnknown),
                )
            }
        }
    }
}

impl From<ApiError> for FirmwareApiError {
    fn from(error: ApiError) -> Self {
        Self::new(error.status, error.code, None)
    }
}

impl From<RepositoryError> for FirmwareApiError {
    fn from(error: RepositoryError) -> Self {
        ApiError::from(error).into()
    }
}

impl IntoResponse for FirmwareApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(FirmwareErrorBody {
                error: self.error,
                phase: self.phase,
            }),
        )
            .into_response()
    }
}

fn log_service_error(message: &'static str, source: anyhow::Error) {
    tracing::error!(error = %format!("{source:#}"), "{message}");
}
