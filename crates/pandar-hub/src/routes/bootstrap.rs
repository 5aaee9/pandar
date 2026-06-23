use axum::{
    Json,
    extract::State,
    extract::rejection::JsonRejection,
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    bootstrap::authorize_bootstrap,
    repositories::{TenantToken, User},
    routes::{ApiError, TenantResponse},
};

#[derive(Debug, Deserialize)]
pub(super) struct BootstrapTenantAdminRequest {
    tenant_slug: String,
    tenant_display_name: String,
    admin_email: String,
    admin_display_name: String,
    api_token_name: String,
}

#[derive(Debug, Serialize)]
pub(super) struct BootstrapTenantAdminResponse {
    tenant: TenantResponse,
    user: UserResponse,
    tenant_token: TenantTokenResponse,
}

#[derive(Debug, Serialize)]
struct UserResponse {
    id: String,
    tenant_id: String,
    email: String,
    display_name: String,
    role: &'static str,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct TenantTokenResponse {
    id: String,
    tenant_id: String,
    name: String,
    scopes: Vec<&'static str>,
    created_by_user_id: Option<String>,
    token: String,
    created_at: String,
    last_used_at: Option<String>,
    expires_at: Option<String>,
    revoked_at: Option<String>,
}

pub(super) async fn create_tenant_admin(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<BootstrapTenantAdminRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<BootstrapTenantAdminResponse>), ApiError> {
    authorize_bootstrap(&state, &headers)?;
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    if payload.tenant_slug.trim().is_empty()
        || payload.tenant_display_name.trim().is_empty()
        || payload.admin_email.trim().is_empty()
        || payload.admin_display_name.trim().is_empty()
        || payload.api_token_name.trim().is_empty()
    {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "bad_request"));
    }

    let bootstrapped = state
        .auth()
        .bootstrap_tenant_admin_with_plaintext_token(
            payload.tenant_slug,
            payload.tenant_display_name,
            payload.admin_email,
            payload.admin_display_name,
            payload.api_token_name,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(BootstrapTenantAdminResponse {
            tenant: TenantResponse::from(bootstrapped.tenant),
            user: UserResponse::from(bootstrapped.user),
            tenant_token: TenantTokenResponse::new(
                bootstrapped.tenant_token,
                bootstrapped.plaintext_token,
            ),
        }),
    ))
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            tenant_id: user.tenant_id.to_string(),
            email: user.email,
            display_name: user.display_name,
            role: user.role.as_str(),
            created_at: user.created_at,
        }
    }
}

impl TenantTokenResponse {
    fn new(token: TenantToken, plaintext_token: String) -> Self {
        Self {
            id: token.id,
            tenant_id: token.tenant_id.to_string(),
            name: token.name,
            scopes: token
                .scopes
                .into_iter()
                .map(|scope| scope.as_str())
                .collect(),
            created_by_user_id: token.created_by_user_id,
            token: plaintext_token,
            created_at: token.created_at,
            last_used_at: token.last_used_at,
            expires_at: token.expires_at,
            revoked_at: token.revoked_at,
        }
    }
}
