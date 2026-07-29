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

use super::{
    UserIdentityListResponse, UserIdentityResponse, UserListResponse, UserResponse, parse_user_role,
};

#[derive(Debug, Deserialize)]
pub(in crate::routes) struct UpdateUserRoleRequest {
    role: String,
}

pub(in crate::routes) async fn list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> Result<Json<UserListResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant(&state, &headers, tenant_id, UserRole::TenantAdmin).await?;
    let (users, identities) = tokio::try_join!(
        state.auth().list_users_for_tenant(tenant_id),
        state.auth().list_external_identities_for_tenant(tenant_id),
    )?;
    let users = users.into_iter().map(UserResponse::from).collect();
    let identities = identities
        .into_iter()
        .map(UserIdentityResponse::from)
        .collect();

    Ok(Json(UserListResponse { users, identities }))
}

pub(in crate::routes) async fn update_user_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, user_id)): Path<(String, String)>,
    payload: Result<Json<UpdateUserRoleRequest>, JsonRejection>,
) -> Result<Json<UserResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let auth = auth::authorize_tenant_admin_principal(&state, &headers, tenant_id).await?;
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    if payload.role.trim().is_empty() {
        return Err(ApiError::bad_request("bad_request"));
    }
    let role = parse_user_role(&payload.role)?;

    let user = state
        .auth()
        .update_user_role_with_audit(tenant_id, &user_id, role, auth::audit_actor(&auth))
        .await?;

    Ok(Json(UserResponse::from(user)))
}

pub(in crate::routes) async fn list_user_identities(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, user_id)): Path<(String, String)>,
) -> Result<Json<UserIdentityListResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant(&state, &headers, tenant_id, UserRole::TenantAdmin).await?;
    let identities = state
        .auth()
        .list_external_identities_for_user(tenant_id, &user_id)
        .await?
        .into_iter()
        .map(UserIdentityResponse::from)
        .collect();

    Ok(Json(UserIdentityListResponse { identities }))
}

pub(in crate::routes) async fn remove_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, user_id)): Path<(String, String)>,
) -> Result<Json<UserResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let principal = auth::authorize_tenant_admin_principal(&state, &headers, tenant_id).await?;
    if let crate::repositories::AuthenticatedPrincipal::User(authenticated) = &principal
        && authenticated.user.id == user_id
    {
        return Err(ApiError::new(StatusCode::CONFLICT, "cannot_remove_self"));
    }
    let user = state
        .auth()
        .remove_user_with_audit(tenant_id, &user_id, auth::audit_actor(&principal))
        .await?;

    Ok(Json(UserResponse::from(user)))
}
