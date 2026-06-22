use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    extract::rejection::JsonRejection,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pandar_core::{Agent, Tenant, TenantId};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    repositories::{RepositoryError, UserRole},
};

mod admin;
mod auth;
mod bootstrap;
pub(crate) mod jobs;
mod printer_events;
mod printers;
mod provisioning;

pub fn router(state: AppState) -> Router {
    let body_limit = state
        .job_storage()
        .max_artifact_bytes()
        .saturating_mul(2)
        .saturating_add(4096);

    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/summary", get(admin::summary))
        .route(
            "/api/v1/tenants",
            get(admin::list_tenants).post(admin::create_tenant),
        )
        .route(
            "/api/v1/bootstrap/tenant-admin",
            post(bootstrap::create_tenant_admin),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agents",
            get(list_agents).post(create_agent),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/users",
            get(provisioning::list_users).post(provisioning::create_user),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/users/{user_id}/role",
            axum::routing::patch(provisioning::update_user_role),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/users/{user_id}/identities",
            get(provisioning::list_user_identities).post(provisioning::link_user_identity),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/users/{user_id}/api-tokens",
            get(provisioning::list_api_tokens).post(provisioning::create_api_token),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/api-tokens/{token_id}",
            axum::routing::delete(provisioning::revoke_api_token),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agent-pairings",
            post(provisioning::create_agent_pairing),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/printers",
            get(printers::list_printers),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/printers/{printer_id}",
            get(printers::get_printer),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs",
            post(jobs::create_job),
        )
        .route("/api/v1/tenants/{tenant_id}/jobs", get(jobs::list_jobs))
        .route(
            "/api/v1/tenants/{tenant_id}/jobs/{job_id}",
            get(jobs::get_job),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agents/{agent_id}/refresh-printers",
            post(printers::refresh_printers),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agents/{agent_id}/discover-printers",
            post(printers::discover_printers),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agents/{agent_id}/diagnose-printer",
            post(printers::diagnose_printer),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/commands/{command_id}",
            get(printers::get_command),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/printer-events",
            get(printer_events::printer_events),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/printer-events/tickets",
            post(printer_events::create_printer_event_ticket),
        )
        .layer(DefaultBodyLimit::max(body_limit))
        .with_state(state)
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct HubSummary {
    tenants: i64,
    agents: i64,
    printers: i64,
    commands: i64,
}

#[derive(Debug, Serialize)]
pub(super) struct TenantResponse {
    id: String,
    slug: String,
    display_name: String,
    created_at: String,
}

#[derive(Debug, Serialize)]
pub(super) struct TenantListResponse {
    tenants: Vec<TenantResponse>,
}

#[derive(Debug, Deserialize)]
struct CreateAgentRequest {
    name: String,
}

#[derive(Debug, Serialize)]
pub(super) struct AgentResponse {
    id: String,
    tenant_id: String,
    name: String,
    status: String,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct AgentListResponse {
    agents: Vec<AgentResponse>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: &'static str,
}

async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn create_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    payload: Result<Json<CreateAgentRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AgentResponse>), ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let auth = auth::authorize_tenant(&state, &headers, tenant_id, UserRole::TenantAdmin).await?;
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "bad_request"));
    }

    let agent = state
        .agents()
        .create_with_audit(tenant_id, payload.name, auth.user.id)
        .await?;

    Ok((StatusCode::CREATED, Json(AgentResponse::from(agent))))
}

async fn list_agents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> Result<Json<AgentListResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant(&state, &headers, tenant_id, UserRole::Viewer).await?;
    let agents = state
        .agents()
        .list_for_tenant(tenant_id)
        .await?
        .into_iter()
        .map(AgentResponse::from)
        .collect();

    Ok(Json(AgentListResponse { agents }))
}

pub(super) fn parse_tenant_id(value: &str) -> Result<TenantId, ApiError> {
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
pub(super) struct ApiError {
    status: StatusCode,
    code: &'static str,
}

impl ApiError {
    pub(super) fn new(status: StatusCode, code: &'static str) -> Self {
        Self { status, code }
    }

    pub(super) fn bad_request(code: &'static str) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code)
    }

    pub(super) fn not_found(code: &'static str) -> Self {
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
            RepositoryError::MissingAgent => Self::new(StatusCode::NOT_FOUND, "agent_not_found"),
            RepositoryError::MissingCommand => {
                Self::new(StatusCode::NOT_FOUND, "command_not_found")
            }
            RepositoryError::MissingPrinter => {
                Self::new(StatusCode::NOT_FOUND, "printer_not_found")
            }
            RepositoryError::MissingJob => Self::new(StatusCode::NOT_FOUND, "job_not_found"),
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
            RepositoryError::InvalidPersistedUserRole(role) => {
                tracing::error!(%role, "invalid persisted user role");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
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

#[cfg(test)]
mod tests;
