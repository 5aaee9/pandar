use axum::{
    Json,
    extract::rejection::JsonRejection,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    repositories::UserRole,
    routes::{AgentResponse, ApiError, auth, parse_tenant_id},
};

#[derive(Debug, Deserialize)]
pub(in crate::routes) struct CreateAgentRequest {
    name: String,
}

#[derive(Debug, Serialize)]
pub(in crate::routes) struct AgentListResponse {
    agents: Vec<AgentResponse>,
}

pub(in crate::routes) async fn create_agent(
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

pub(in crate::routes) async fn list_agents(
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
