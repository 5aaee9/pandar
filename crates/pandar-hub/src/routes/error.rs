use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use pandar_core::{Agent, Tenant, TenantId};
use serde::Serialize;

use super::{AgentResponse, TenantResponse};
use crate::repositories::RepositoryError;

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: &'static str,
}

pub(crate) fn parse_tenant_id(value: &str) -> Result<TenantId, ApiError> {
    TenantId::parse(value).map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid_tenant_id"))
}

impl From<Tenant> for TenantResponse {
    fn from(tenant: Tenant) -> Self {
        Self {
            id: tenant.id.to_string(),
            slug: tenant.slug,
            display_name: tenant.display_name,
            created_at: tenant.created_at,
        }
    }
}

impl From<Agent> for AgentResponse {
    fn from(agent: Agent) -> Self {
        Self {
            id: agent.id.to_string(),
            tenant_id: agent.tenant_id.to_string(),
            name: agent.name,
            status: agent.status.to_string(),
            created_at: agent.created_at,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ApiError {
    pub(crate) status: StatusCode,
    pub(crate) code: &'static str,
}

impl ApiError {
    pub(crate) fn new(status: StatusCode, code: &'static str) -> Self {
        Self { status, code }
    }

    pub(crate) fn bad_request(code: &'static str) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code)
    }

    pub(crate) fn not_found(code: &'static str) -> Self {
        Self::new(StatusCode::NOT_FOUND, code)
    }
}

impl From<RepositoryError> for ApiError {
    fn from(err: RepositoryError) -> Self {
        match err {
            RepositoryError::DuplicateTenantSlug => {
                Self::new(StatusCode::CONFLICT, "tenant_slug_exists")
            }
            RepositoryError::DuplicateAgentName => {
                Self::new(StatusCode::CONFLICT, "agent_name_exists")
            }
            RepositoryError::DuplicateApiTokenName => {
                Self::new(StatusCode::CONFLICT, "api_token_name_exists")
            }
            RepositoryError::DuplicateApiTokenHash => {
                Self::new(StatusCode::CONFLICT, "api_token_hash_exists")
            }
            RepositoryError::DuplicateTenantTokenHash => {
                Self::new(StatusCode::CONFLICT, "tenant_token_hash_exists")
            }
            RepositoryError::DuplicatePluginLoginTicketHash => {
                Self::new(StatusCode::CONFLICT, "plugin_login_ticket_hash_exists")
            }
            RepositoryError::DuplicateJoinLinkHash => {
                Self::new(StatusCode::CONFLICT, "join_link_hash_exists")
            }
            RepositoryError::DuplicateExternalIdentity => {
                Self::new(StatusCode::CONFLICT, "external_identity_exists")
            }
            RepositoryError::DuplicateUserExternalIdentity => Self::new(
                StatusCode::CONFLICT,
                "user_external_identity_provider_exists",
            ),
            RepositoryError::DuplicateUserEmail => {
                Self::new(StatusCode::CONFLICT, "user_email_exists")
            }
            RepositoryError::MissingTenant => Self::new(StatusCode::NOT_FOUND, "tenant_not_found"),
            RepositoryError::MissingUser => Self::new(StatusCode::NOT_FOUND, "user_not_found"),
            RepositoryError::MissingApiToken => {
                Self::new(StatusCode::NOT_FOUND, "api_token_not_found")
            }
            RepositoryError::MissingTenantToken => {
                Self::new(StatusCode::NOT_FOUND, "tenant_token_not_found")
            }
            RepositoryError::MissingPluginLoginTicket => {
                Self::new(StatusCode::UNAUTHORIZED, "invalid_login_ticket")
            }
            RepositoryError::InvalidJoinLink => {
                Self::new(StatusCode::NOT_FOUND, "invalid_join_link")
            }
            RepositoryError::JoinLinkEmailMismatch => {
                Self::new(StatusCode::FORBIDDEN, "join_link_email_mismatch")
            }
            RepositoryError::MissingAgent => Self::new(StatusCode::NOT_FOUND, "agent_not_found"),
            RepositoryError::AgentOnline => Self::new(StatusCode::CONFLICT, "agent_online"),
            RepositoryError::AgentSessionNotCurrent => {
                Self::new(StatusCode::CONFLICT, "agent_session_not_current")
            }
            RepositoryError::MissingCommand => {
                Self::new(StatusCode::NOT_FOUND, "command_not_found")
            }
            RepositoryError::MissingPrinter => {
                Self::new(StatusCode::NOT_FOUND, "printer_not_found")
            }
            RepositoryError::MissingJob => Self::new(StatusCode::NOT_FOUND, "job_not_found"),
            RepositoryError::JobNotClearable => {
                Self::new(StatusCode::CONFLICT, "job_not_clearable")
            }
            RepositoryError::CommandOwnershipMismatch => {
                Self::new(StatusCode::FORBIDDEN, "command_ownership_mismatch")
            }
            RepositoryError::InvalidCommandTransition { .. } => {
                Self::new(StatusCode::CONFLICT, "invalid_command_transition")
            }
            RepositoryError::InvalidPersistedStatus(status) => {
                tracing::error!(%status, "invalid persisted agent status");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
            }
            RepositoryError::InvalidPersistedCommandStatus(status) => {
                tracing::error!(%status, "invalid persisted command status");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
            }
            RepositoryError::InvalidPersistedJobStatus(status) => {
                tracing::error!(%status, "invalid persisted job status");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
            }
            RepositoryError::InvalidPersistedPrintStatus(status) => {
                tracing::error!(%status, "invalid persisted print status");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
            }
            RepositoryError::InvalidPersistedArtifactMetadata(err) => {
                tracing::error!(error = %format!("{err:#}"), "invalid persisted artifact metadata");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
            }
            RepositoryError::InvalidPersistedStudioMetadata(err) => {
                tracing::error!(error = %format!("{err:#}"), "invalid persisted Studio metadata");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
            }
            RepositoryError::InvalidPersistedUserRole(role) => {
                tracing::error!(%role, "invalid persisted user role");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
            }
            RepositoryError::InvalidTokenScope(scope) => {
                tracing::error!(%scope, "invalid tenant token scope");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
            }
            RepositoryError::InvalidPluginRedirectUrl => {
                Self::new(StatusCode::BAD_REQUEST, "invalid_redirect_url")
            }
            RepositoryError::RetryNotSafe => Self::new(StatusCode::CONFLICT, "retry_not_safe"),
            RepositoryError::ReprintNotAllowed => {
                Self::new(StatusCode::CONFLICT, "reprint_not_allowed")
            }
            RepositoryError::StudioSubmissionIdExhausted => {
                Self::new(StatusCode::CONFLICT, "studio_submission_id_exhausted")
            }
            RepositoryError::StudioCancellationTooLate => {
                Self::new(StatusCode::CONFLICT, "studio_cancellation_too_late")
            }
            RepositoryError::PrinterControlUnavailable => {
                Self::new(StatusCode::BAD_REQUEST, "printer_control_unavailable")
            }
            RepositoryError::InvalidPrinterControl => {
                Self::new(StatusCode::BAD_REQUEST, "invalid_printer_control")
            }
            RepositoryError::Database(err) => {
                tracing::error!(error = %format!("{err:#}"), "repository database error");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(ErrorResponse { error: self.code })).into_response()
    }
}
