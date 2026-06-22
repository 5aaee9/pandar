use axum::{
    Json,
    extract::rejection::JsonRejection,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::Deserialize;

use crate::{
    AppState,
    repositories::UserRole,
    routes::{AgentResponse, ApiError, auth, parse_tenant_id},
};

use super::AgentPairingResponse;

#[derive(Debug, Deserialize)]
pub(in crate::routes) struct CreateAgentPairingRequest {
    name: String,
}

pub(in crate::routes) async fn create_agent_pairing(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    payload: Result<Json<CreateAgentPairingRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AgentPairingResponse>), ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let auth = auth::authorize_tenant(&state, &headers, tenant_id, UserRole::TenantAdmin).await?;
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::bad_request("bad_request"));
    }

    let agent = state
        .agents()
        .create_pairing_bundle_with_audit(tenant_id, payload.name, auth.user.id)
        .await?;

    let agent_env = format!(
        "PANDAR_TENANT_ID={}\nPANDAR_AGENT_ID={}\nPANDAR_AGENT_NAME={}\n",
        agent.tenant_id, agent.id, agent.name
    );

    Ok((
        StatusCode::CREATED,
        Json(AgentPairingResponse {
            agent: AgentResponse::from(agent),
            agent_env,
        }),
    ))
}
