use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    extract::rejection::JsonRejection,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pandar_core::{Agent, Tenant, TenantId};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    AppState,
    repositories::{AuditEventListQuery, RepositoryError, UserRole},
};

mod admin;
mod auth;
mod bootstrap;
pub(crate) mod jobs;
mod plugin;
mod printer_events;
mod printers;
mod provisioning;
mod tenant_tokens;

pub fn router(state: AppState) -> Router {
    let body_limit = state
        .job_storage()
        .max_artifact_bytes()
        .saturating_mul(2)
        .saturating_add(4096);

    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
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
            "/api/v1/tenants/{tenant_id}/tenant-tokens",
            get(tenant_tokens::list_tenant_tokens).post(tenant_tokens::create_tenant_token),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/tenant-tokens/{token_id}",
            axum::routing::delete(tenant_tokens::revoke_tenant_token),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/tenant-tokens/{token_id}/rotate",
            post(tenant_tokens::rotate_tenant_token),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/plugin/login-tickets",
            post(plugin::create_login_ticket),
        )
        .route(
            "/api/v1/plugin/login-tickets/exchange",
            post(plugin::exchange_login_ticket),
        )
        .route("/api/v1/plugin/printers", get(plugin::list_printers))
        .route("/api/v1/plugin/jobs", get(plugin::list_jobs))
        .route("/api/v1/plugin/prints", post(plugin::create_print))
        .route(
            "/api/v1/tenants/{tenant_id}/audit-events",
            get(list_audit_events),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agent-pairings",
            post(provisioning::create_agent_pairing),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agents/{agent_id}/credential:rotate",
            post(provisioning::rotate_agent_credential),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/agents/{agent_id}/credential:revoke",
            post(provisioning::revoke_agent_credential),
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
            "/api/v1/tenants/{tenant_id}/jobs/{job_id}/retry-dispatch",
            post(jobs::retry_dispatch),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/jobs/{job_id}/reprint",
            post(jobs::reprint),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/jobs/{job_id}/duplicate",
            post(jobs::duplicate),
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

#[derive(Debug, Deserialize)]
struct AuditEventsQuery {
    limit: Option<usize>,
    before: Option<String>,
    action: Option<String>,
}

#[derive(Debug, Serialize)]
struct AuditEventResponse {
    id: String,
    tenant_id: String,
    actor_type: String,
    user_id: Option<String>,
    action: String,
    target_type: String,
    target_id: Option<String>,
    metadata: Value,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct AuditEventListResponse {
    audit_events: Vec<AuditEventResponse>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: &'static str,
}

async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn readyz(
    State(state): State<AppState>,
) -> (StatusCode, Json<crate::readiness::ReadinessResponse>) {
    let readiness = crate::readiness::check(&state).await;
    let status = if readiness.status == "ready" {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(readiness))
}

async fn metrics(State(state): State<AppState>) -> Result<Response, ApiError> {
    let body = crate::metrics_export::prometheus_metrics(&state)
        .await
        .map_err(|err| {
            tracing::error!(error = %format!("{err:#}"), "failed to render metrics");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
        })?;
    Ok((
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        body,
    )
        .into_response())
}

async fn create_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    payload: Result<Json<CreateAgentRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AgentResponse>), ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let auth = auth::authorize_tenant_admin_principal(&state, &headers, tenant_id).await?;
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "bad_request"));
    }

    let agent = state
        .agents()
        .create_with_audit(tenant_id, payload.name, auth::audit_actor(&auth))
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

async fn list_audit_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    Query(query): Query<AuditEventsQuery>,
) -> Result<Json<AuditEventListResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant_admin_principal(&state, &headers, tenant_id).await?;
    let limit = query.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("invalid_limit"));
    }
    if let Some(before) = &query.before {
        OffsetDateTime::parse(before, &Rfc3339)
            .map_err(|_| ApiError::bad_request("invalid_before"))?;
    }

    let audit_events = state
        .audit_events()
        .list_for_tenant_query(
            tenant_id,
            AuditEventListQuery {
                limit: limit as u64,
                before: query.before,
                action: query.action,
            },
        )
        .await?
        .into_iter()
        .map(AuditEventResponse::from)
        .collect();

    Ok(Json(AuditEventListResponse { audit_events }))
}

pub(super) fn parse_tenant_id(value: &str) -> Result<TenantId, ApiError> {
    TenantId::parse(value).map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid_tenant_id"))
}

fn audit_metadata(metadata_json: &str, event_id: &str) -> Value {
    let metadata = match serde_json::from_str::<Value>(metadata_json) {
        Ok(Value::Object(map)) => Value::Object(map),
        Ok(_) => Value::Object(Map::new()),
        Err(err) => {
            let error = anyhow::Error::new(err).context(format!(
                "failed to parse audit metadata for event {event_id}"
            ));
            tracing::error!(error = %format!("{error:#}"), "invalid persisted audit metadata");
            Value::Object(Map::new())
        }
    };
    redact_audit_metadata(metadata)
}

fn redact_audit_metadata(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter_map(|(key, value)| {
                    if is_forbidden_audit_metadata_key(&key) {
                        None
                    } else {
                        Some((key, redact_audit_metadata(value)))
                    }
                })
                .collect(),
        ),
        Value::Array(values) => {
            Value::Array(values.into_iter().map(redact_audit_metadata).collect())
        }
        other => other,
    }
}

fn is_forbidden_audit_metadata_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace(['-', ' '], "_");
    normalized.contains("plaintext_token")
        || normalized.contains("token_hash")
        || normalized.contains("agent_credential")
        || normalized.contains("credential_hash")
        || normalized.contains("plugin_ticket")
        || normalized == "ticket"
        || normalized.ends_with("_ticket")
        || normalized.contains("plaintext_ticket")
        || normalized.contains("ticket_hash")
        || normalized.contains("bambu_access_code")
        || normalized.contains("artifact_storage_path")
        || normalized == "storage_path"
        || normalized.contains("bearer")
        || normalized.contains("authorization")
        || normalized == "subject"
        || normalized.contains("external_subject")
        || normalized.contains("provider_subject")
        || normalized.contains("external_provider_subject")
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

impl From<crate::repositories::AuditEvent> for AuditEventResponse {
    fn from(event: crate::repositories::AuditEvent) -> Self {
        let metadata = audit_metadata(&event.metadata_json, &event.id);
        Self {
            id: event.id,
            tenant_id: event.tenant_id.to_string(),
            actor_type: event.actor_type,
            user_id: event.user_id,
            action: event.action,
            target_type: event.target_type,
            target_id: event.target_id,
            metadata,
            created_at: event.created_at,
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
            RepositoryError::DuplicateTenantTokenHash => {
                Self::new(StatusCode::CONFLICT, "tenant_token_hash_exists")
            }
            RepositoryError::DuplicatePluginLoginTicketHash => {
                Self::new(StatusCode::CONFLICT, "plugin_login_ticket_hash_exists")
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
