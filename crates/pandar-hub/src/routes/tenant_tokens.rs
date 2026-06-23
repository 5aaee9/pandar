use axum::{
    Json,
    extract::rejection::JsonRejection,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    AppState,
    repositories::{TenantToken, TenantTokenScope},
    routes::{ApiError, auth, parse_tenant_id},
};

#[derive(Debug, Deserialize)]
pub(in crate::routes) struct CreateTenantTokenRequest {
    name: String,
    scopes: Vec<String>,
    expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(in crate::routes) struct RotateTenantTokenRequest {
    expires_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub(in crate::routes) struct TenantTokenResponse {
    id: String,
    tenant_id: String,
    name: String,
    scopes: Vec<&'static str>,
    created_by_user_id: Option<String>,
    created_at: String,
    last_used_at: Option<String>,
    expires_at: Option<String>,
    revoked_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub(in crate::routes) struct TenantTokenWithPlaintextResponse {
    tenant_token: TenantTokenResponse,
    token: String,
}

#[derive(Debug, Serialize)]
pub(in crate::routes) struct RotatedTenantTokenResponse {
    tenant_token: TenantTokenResponse,
    token: String,
    rotated_from_token_id: String,
}

#[derive(Debug, Serialize)]
pub(in crate::routes) struct TenantTokenListResponse {
    tenant_tokens: Vec<TenantTokenResponse>,
}

pub(super) async fn list_tenant_tokens(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> Result<Json<TenantTokenListResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant_admin_principal(&state, &headers, tenant_id).await?;
    let tenant_tokens = state
        .auth()
        .list_tenant_tokens(tenant_id)
        .await?
        .into_iter()
        .map(TenantTokenResponse::from)
        .collect();

    Ok(Json(TenantTokenListResponse { tenant_tokens }))
}

pub(super) async fn create_tenant_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    payload: Result<Json<CreateTenantTokenRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<TenantTokenWithPlaintextResponse>), ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let principal = auth::authorize_tenant_admin_principal(&state, &headers, tenant_id).await?;
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::bad_request("bad_request"));
    }
    let scopes = parse_scopes(payload.scopes)?;
    let expires_at = validate_expires_at(payload.expires_at)?;
    let created = state
        .auth()
        .create_tenant_token_with_audit(
            tenant_id,
            payload.name,
            scopes,
            expires_at,
            auth::audit_actor(&principal),
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(TenantTokenWithPlaintextResponse {
            tenant_token: TenantTokenResponse::from(created.token),
            token: created.plaintext_token,
        }),
    ))
}

pub(super) async fn revoke_tenant_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, token_id)): Path<(String, String)>,
) -> Result<Json<RevokeTenantTokenResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let principal = auth::authorize_tenant_admin_principal(&state, &headers, tenant_id).await?;
    let token = state
        .auth()
        .revoke_tenant_token_with_audit(tenant_id, &token_id, auth::audit_actor(&principal))
        .await?;

    Ok(Json(RevokeTenantTokenResponse {
        tenant_token: TenantTokenResponse::from(token),
    }))
}

pub(super) async fn rotate_tenant_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, token_id)): Path<(String, String)>,
    payload: Result<Json<RotateTenantTokenRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<RotatedTenantTokenResponse>), ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let principal = auth::authorize_tenant_admin_principal(&state, &headers, tenant_id).await?;
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    let expires_at = validate_expires_at(payload.expires_at)?;
    let rotated = state
        .auth()
        .rotate_tenant_token_with_audit(
            tenant_id,
            &token_id,
            expires_at,
            auth::audit_actor(&principal),
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(RotatedTenantTokenResponse {
            tenant_token: TenantTokenResponse::from(rotated.token),
            token: rotated.plaintext_token,
            rotated_from_token_id: token_id,
        }),
    ))
}

fn parse_scopes(scopes: Vec<String>) -> Result<Vec<TenantTokenScope>, ApiError> {
    scopes
        .into_iter()
        .map(|scope| {
            TenantTokenScope::parse(&scope)
                .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid_scope"))
        })
        .collect()
}

fn validate_expires_at(expires_at: Option<String>) -> Result<Option<String>, ApiError> {
    if let Some(value) = &expires_at {
        OffsetDateTime::parse(value, &Rfc3339)
            .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid_expires_at"))?;
    }
    Ok(expires_at)
}

impl From<TenantToken> for TenantTokenResponse {
    fn from(token: TenantToken) -> Self {
        Self {
            id: token.id,
            tenant_id: token.tenant_id.to_string(),
            name: token.name,
            scopes: token
                .scopes
                .into_iter()
                .map(TenantTokenScope::as_str)
                .collect(),
            created_by_user_id: token.created_by_user_id,
            created_at: token.created_at,
            last_used_at: token.last_used_at,
            expires_at: token.expires_at,
            revoked_at: token.revoked_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub(in crate::routes) struct RevokeTenantTokenResponse {
    tenant_token: TenantTokenResponse,
}
