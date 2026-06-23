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
pub(in crate::routes) struct CreateUserRequest {
    email: String,
    display_name: String,
    role: String,
}

#[derive(Debug, Deserialize)]
pub(in crate::routes) struct UpdateUserRoleRequest {
    role: String,
}

#[derive(Debug, Deserialize)]
pub(in crate::routes) struct LinkUserIdentityRequest {
    provider: String,
    subject: String,
}

pub(in crate::routes) async fn list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> Result<Json<UserListResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant(&state, &headers, tenant_id, UserRole::TenantAdmin).await?;
    let users = state
        .auth()
        .list_users_for_tenant(tenant_id)
        .await?
        .into_iter()
        .map(UserResponse::from)
        .collect();

    Ok(Json(UserListResponse { users }))
}

pub(in crate::routes) async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    payload: Result<Json<CreateUserRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<UserResponse>), ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let auth = auth::authorize_tenant_admin_principal(&state, &headers, tenant_id).await?;
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    if payload.email.trim().is_empty()
        || payload.display_name.trim().is_empty()
        || payload.role.trim().is_empty()
    {
        return Err(ApiError::bad_request("bad_request"));
    }
    let role = parse_user_role(&payload.role)?;

    let user = state
        .auth()
        .create_user_with_audit(
            tenant_id,
            payload.email,
            payload.display_name,
            role,
            auth::audit_actor(&auth),
        )
        .await?;

    Ok((StatusCode::CREATED, Json(UserResponse::from(user))))
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

pub(in crate::routes) async fn link_user_identity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, user_id)): Path<(String, String)>,
    payload: Result<Json<LinkUserIdentityRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<UserIdentityResponse>), ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let auth = auth::authorize_tenant_admin_principal(&state, &headers, tenant_id).await?;
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    if payload.provider.trim().is_empty() || payload.subject.trim().is_empty() {
        return Err(ApiError::bad_request("bad_request"));
    }

    let identity = state
        .auth()
        .link_external_identity_with_audit(
            tenant_id,
            &user_id,
            payload.provider,
            payload.subject,
            auth::audit_actor(&auth),
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(UserIdentityResponse::from(identity)),
    ))
}
