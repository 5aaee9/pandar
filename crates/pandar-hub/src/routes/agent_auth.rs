use axum::http::{HeaderMap, StatusCode, header};

use crate::{AppState, repositories::hash_secret, routes::ApiError};

pub(in crate::routes) struct AuthorizedAgent {
    pub tenant_id: pandar_core::TenantId,
}

pub(in crate::routes) async fn authorize_agent(
    state: &AppState,
    headers: &HeaderMap,
    agent_id: pandar_core::AgentId,
) -> Result<AuthorizedAgent, ApiError> {
    let credential = bearer_token(headers)?;
    let credential_hash = hash_secret(credential);
    let records = state
        .agents()
        .credential_records_by_hash(&credential_hash)
        .await?;
    let [actual] = records.as_slice() else {
        return Err(unauthorized());
    };
    if actual.credential_revoked_at.is_some() {
        return Err(unauthorized());
    }
    if actual.agent.id != agent_id {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "forbidden"));
    }

    Ok(AuthorizedAgent {
        tenant_id: actual.agent.tenant_id,
    })
}

fn bearer_token(headers: &HeaderMap) -> Result<&str, ApiError> {
    let Some(raw) = headers.get(header::AUTHORIZATION) else {
        return Err(unauthorized());
    };
    let Ok(value) = raw.to_str() else {
        return Err(unauthorized());
    };
    value
        .strip_prefix("Bearer ")
        .filter(|credential| !credential.is_empty())
        .ok_or_else(unauthorized)
}

fn unauthorized() -> ApiError {
    ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized")
}
