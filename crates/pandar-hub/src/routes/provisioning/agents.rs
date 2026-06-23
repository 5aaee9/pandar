use axum::{
    Json,
    extract::rejection::JsonRejection,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::Deserialize;

use crate::{
    AppState,
    repositories::{AGENT_CREDENTIAL_PREFIX, generate_secret},
    routes::{AgentResponse, ApiError, auth, parse_tenant_id},
};

use super::{AgentCredentialRotateResponse, AgentPairingResponse};

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
    let auth = auth::authorize_agent_registration(&state, &headers, tenant_id).await?;
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    if invalid_agent_name_for_env(&payload.name) {
        return Err(ApiError::bad_request("bad_request"));
    }

    let agent = state
        .agents()
        .create_pairing_bundle_with_audit(tenant_id, payload.name, auth::audit_actor(&auth))
        .await?;

    let agent_env = format!(
        "PANDAR_TENANT_ID={}\nPANDAR_AGENT_ID={}\nPANDAR_AGENT_NAME={}\nPANDAR_AGENT_CREDENTIAL={}\n",
        agent.agent.tenant_id, agent.agent.id, agent.agent.name, agent.credential
    );

    Ok((
        StatusCode::CREATED,
        Json(AgentPairingResponse {
            agent: AgentResponse::from(agent.agent),
            agent_env,
        }),
    ))
}

pub(in crate::routes) async fn rotate_agent_credential(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, agent_id)): Path<(String, String)>,
) -> Result<Json<AgentCredentialRotateResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let agent_id = pandar_core::AgentId::parse(&agent_id)
        .map_err(|_| ApiError::bad_request("invalid_agent_id"))?;
    let auth = auth::authorize_agent_registration(&state, &headers, tenant_id).await?;

    let credential = generate_secret(AGENT_CREDENTIAL_PREFIX);
    let record = state
        .agents()
        .rotate_credential(tenant_id, agent_id, &credential, auth::audit_actor(&auth))
        .await?;
    state.close_agent(tenant_id, agent_id).await;

    Ok(Json(AgentCredentialRotateResponse {
        agent: AgentResponse::from(record.agent),
        credential,
    }))
}

pub(in crate::routes) async fn revoke_agent_credential(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, agent_id)): Path<(String, String)>,
) -> Result<Json<AgentResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let agent_id = pandar_core::AgentId::parse(&agent_id)
        .map_err(|_| ApiError::bad_request("invalid_agent_id"))?;
    let auth = auth::authorize_agent_registration(&state, &headers, tenant_id).await?;

    let record = state
        .agents()
        .revoke_credential(tenant_id, agent_id, auth::audit_actor(&auth))
        .await?;
    state.close_agent(tenant_id, agent_id).await;

    Ok(Json(AgentResponse::from(record.agent)))
}

fn invalid_agent_name_for_env(name: &str) -> bool {
    name.trim().is_empty() || name.contains(['\r', '\n', '\0'])
}
