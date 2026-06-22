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
    routes::{ApiError, auth, parse_tenant_id},
};

use super::{ApiTokenListResponse, ApiTokenResponse, ApiTokenWithPlaintextResponse};

#[derive(Debug, Deserialize)]
pub(in crate::routes) struct CreateApiTokenRequest {
    name: String,
}

pub(in crate::routes) async fn list_api_tokens(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, user_id)): Path<(String, String)>,
) -> Result<Json<ApiTokenListResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant(&state, &headers, tenant_id, UserRole::TenantAdmin).await?;
    let api_tokens = state
        .auth()
        .list_api_tokens_for_user(tenant_id, &user_id)
        .await?
        .into_iter()
        .map(ApiTokenResponse::from)
        .collect();

    Ok(Json(ApiTokenListResponse { api_tokens }))
}

pub(in crate::routes) async fn create_api_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, user_id)): Path<(String, String)>,
    payload: Result<Json<CreateApiTokenRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<ApiTokenWithPlaintextResponse>), ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let auth = auth::authorize_tenant(&state, &headers, tenant_id, UserRole::TenantAdmin).await?;
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::bad_request("bad_request"));
    }

    let plaintext_token = format!("pandar_{}", uuid::Uuid::new_v4().simple());
    let token = state
        .auth()
        .create_api_token_with_audit(
            tenant_id,
            &user_id,
            payload.name,
            &plaintext_token,
            auth.user.id,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiTokenWithPlaintextResponse::new(token, plaintext_token)),
    ))
}

pub(in crate::routes) async fn revoke_api_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, token_id)): Path<(String, String)>,
) -> Result<Json<ApiTokenResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let auth = auth::authorize_tenant(&state, &headers, tenant_id, UserRole::TenantAdmin).await?;
    let token = state
        .auth()
        .revoke_api_token_with_audit(tenant_id, &token_id, auth.user.id)
        .await?;

    Ok(Json(ApiTokenResponse::from(token)))
}
