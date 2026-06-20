use axum::{
    Json, Router,
    extract::rejection::JsonRejection,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use pandar_core::{Agent, Tenant, TenantId};
use serde::{Deserialize, Serialize};

use crate::{AppState, repositories::RepositoryError};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/summary", get(summary))
        .route("/api/v1/tenants", get(list_tenants).post(create_tenant))
        .route(
            "/api/v1/tenants/{tenant_id}/agents",
            get(list_agents).post(create_agent),
        )
        .with_state(state)
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct HubSummary {
    tenants: i64,
    agents: i64,
    printers: i64,
    commands: i64,
}

#[derive(Debug, Deserialize)]
struct CreateTenantRequest {
    slug: String,
    display_name: String,
}

#[derive(Debug, Serialize)]
struct TenantResponse {
    id: String,
    slug: String,
    display_name: String,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct TenantListResponse {
    tenants: Vec<TenantResponse>,
}

#[derive(Debug, Deserialize)]
struct CreateAgentRequest {
    name: String,
}

#[derive(Debug, Serialize)]
struct AgentResponse {
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

async fn summary(State(state): State<AppState>) -> Result<Json<HubSummary>, ApiError> {
    Ok(Json(HubSummary {
        tenants: state.tenants().count().await?,
        agents: state.agents().count().await?,
        printers: state.printers().count().await?,
        commands: state.commands().count().await?,
    }))
}

async fn create_tenant(
    State(state): State<AppState>,
    payload: Result<Json<CreateTenantRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<TenantResponse>), ApiError> {
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    if payload.slug.trim().is_empty() || payload.display_name.trim().is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "bad_request"));
    }

    let tenant = state
        .tenants()
        .create(payload.slug, payload.display_name)
        .await?;

    Ok((StatusCode::CREATED, Json(TenantResponse::from(tenant))))
}

async fn list_tenants(State(state): State<AppState>) -> Result<Json<TenantListResponse>, ApiError> {
    let tenants = state
        .tenants()
        .list()
        .await?
        .into_iter()
        .map(TenantResponse::from)
        .collect();

    Ok(Json(TenantListResponse { tenants }))
}

async fn create_agent(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    payload: Result<Json<CreateAgentRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AgentResponse>), ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "bad_request"));
    }

    let agent = state.agents().create(tenant_id, payload.name).await?;

    Ok((StatusCode::CREATED, Json(AgentResponse::from(agent))))
}

async fn list_agents(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
) -> Result<Json<AgentListResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let agents = state
        .agents()
        .list_for_tenant(tenant_id)
        .await?
        .into_iter()
        .map(AgentResponse::from)
        .collect();

    Ok(Json(AgentListResponse { agents }))
}

fn parse_tenant_id(value: &str) -> Result<TenantId, ApiError> {
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
struct ApiError {
    status: StatusCode,
    code: &'static str,
}

impl ApiError {
    fn new(status: StatusCode, code: &'static str) -> Self {
        Self { status, code }
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
            RepositoryError::MissingTenant => Self::new(StatusCode::NOT_FOUND, "tenant_not_found"),
            RepositoryError::InvalidPersistedStatus(status) => {
                tracing::error!(%status, "invalid persisted agent status");
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
