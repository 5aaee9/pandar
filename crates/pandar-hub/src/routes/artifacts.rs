use axum::{
    body::{Body, Bytes},
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use futures_util::stream;
use tokio::io::AsyncReadExt;

use crate::{
    AppState,
    repositories::{AgentArtifactAccess, hash_secret},
    routes::ApiError,
};

pub(in crate::routes) async fn download_agent_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((agent_id, artifact_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let agent_id = pandar_core::AgentId::parse(&agent_id)
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid_agent_id"))?;
    let authorized = authorize_agent(&state, &headers, agent_id).await?;
    let artifact = match state
        .jobs()
        .artifact_access_for_agent(authorized.tenant_id, agent_id, &artifact_id)
        .await?
    {
        AgentArtifactAccess::Allowed(artifact) => artifact,
        AgentArtifactAccess::Forbidden => {
            return Err(ApiError::new(StatusCode::FORBIDDEN, "forbidden"));
        }
        AgentArtifactAccess::NotFound => {
            return Err(ApiError::new(StatusCode::NOT_FOUND, "artifact_not_found"));
        }
    };
    let body = state
        .artifact_storage()
        .open_artifact(&artifact.storage_path)
        .await
        .map_err(|err| {
            tracing::error!(
                artifact_id = %artifact.id,
                error = %crate::routes::plugin::redact_artifact_error(&format!("{err:#}")),
                "failed to open artifact for agent download"
            );
            if state.artifact_storage().is_not_found(&err) {
                ApiError::new(StatusCode::NOT_FOUND, "artifact_not_found")
            } else {
                ApiError::new(StatusCode::BAD_GATEWAY, "artifact_unavailable")
            }
        })?;
    let body = stream::try_unfold(body, |mut body| async move {
        let mut buffer = vec![0; 8192];
        let read = body.read(&mut buffer).await?;
        if read == 0 {
            Ok::<Option<(Bytes, tokio::fs::File)>, std::io::Error>(None)
        } else {
            buffer.truncate(read);
            Ok(Some((Bytes::from(buffer), body)))
        }
    });

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, artifact.content_type)
        .body(Body::from_stream(body))
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"))
}

struct AuthorizedAgent {
    tenant_id: pandar_core::TenantId,
}

async fn authorize_agent(
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
