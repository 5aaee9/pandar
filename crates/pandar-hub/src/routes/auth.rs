use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use pandar_core::TenantId;

use crate::{
    AppState,
    repositories::{AuthenticatedUser, UserRole},
    routes::ApiError,
};

pub(super) async fn authorize_tenant(
    state: &AppState,
    headers: &HeaderMap,
    tenant_id: TenantId,
    required_role: UserRole,
) -> Result<AuthenticatedUser, ApiError> {
    let Some(header) = headers.get(AUTHORIZATION) else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "missing_auth_token",
        ));
    };
    let header = header
        .to_str()
        .map_err(|_| ApiError::new(StatusCode::UNAUTHORIZED, "invalid_auth_token"))?;
    let Some(token) = header.strip_prefix("Bearer ") else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_auth_token",
        ));
    };

    let Some(authenticated) = state.auth().authenticate_bearer(token).await? else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_auth_token",
        ));
    };
    if authenticated.user.tenant_id != tenant_id {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "tenant_forbidden"));
    }
    if !authenticated.user.role.allows(required_role) {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "role_forbidden"));
    }

    Ok(authenticated)
}
