use axum::{
    Json,
    extract::rejection::JsonRejection,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    repositories::{AuditActor, JoinLink, UserRole},
    routes::{ApiError, auth, parse_tenant_id},
};

#[derive(Debug, Deserialize)]
pub(super) struct CreateJoinLinkRequest {
    role: String,
    email: Option<String>,
    email_constraint: Option<String>,
    expires_in_seconds: Option<i64>,
    max_uses: Option<i32>,
}

#[derive(Debug, Serialize)]
pub(super) struct JoinLinkResponse {
    id: String,
    tenant_id: String,
    role: &'static str,
    email_constraint: Option<String>,
    expires_at: String,
    max_uses: i32,
    used_count: i32,
    created_by_user_id: Option<String>,
    revoked_at: Option<String>,
    created_at: String,
}

#[derive(Debug, Serialize)]
pub(super) struct JoinLinkWithPlaintextResponse {
    join_link: JoinLinkResponse,
    token: String,
}

#[derive(Debug, Serialize)]
pub(super) struct JoinLinkListResponse {
    join_links: Vec<JoinLinkResponse>,
}

pub(super) async fn list_join_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> Result<Json<JoinLinkListResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant_admin_user(&state, &headers, tenant_id).await?;
    let join_links = state
        .auth()
        .list_join_links_for_tenant(tenant_id)
        .await?
        .into_iter()
        .map(JoinLinkResponse::from)
        .collect();
    Ok(Json(JoinLinkListResponse { join_links }))
}

pub(super) async fn create_join_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    payload: Result<Json<CreateJoinLinkRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<JoinLinkWithPlaintextResponse>), ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let authenticated = auth::authorize_tenant_admin_user(&state, &headers, tenant_id).await?;
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    let role = UserRole::parse(&payload.role)
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid_user_role"))?;
    let expires_in_seconds = payload.expires_in_seconds.unwrap_or(7 * 24 * 60 * 60);
    if expires_in_seconds <= 0 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid_join_link_ttl",
        ));
    }
    let max_uses = payload.max_uses.unwrap_or(1);
    if max_uses <= 0 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid_join_link_max_uses",
        ));
    }
    let created = state
        .auth()
        .create_join_link_with_audit(
            tenant_id,
            role,
            payload.email_constraint.or(payload.email),
            expires_in_seconds,
            max_uses,
            AuditActor::user(authenticated.user.id),
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(JoinLinkWithPlaintextResponse {
            join_link: JoinLinkResponse::from(created.join_link),
            token: created.plaintext_token,
        }),
    ))
}

pub(super) async fn revoke_join_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, join_link_id)): Path<(String, String)>,
) -> Result<Json<JoinLinkResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    let authenticated = auth::authorize_tenant_admin_user(&state, &headers, tenant_id).await?;
    let join_link = state
        .auth()
        .revoke_join_link_with_audit(
            tenant_id,
            &join_link_id,
            AuditActor::user(authenticated.user.id),
        )
        .await?;
    Ok(Json(JoinLinkResponse::from(join_link)))
}

impl JoinLinkResponse {
    pub(super) fn from_join_link(join_link: JoinLink) -> Self {
        Self {
            id: join_link.id,
            tenant_id: join_link.tenant_id,
            role: join_link.role.as_str(),
            email_constraint: join_link.email_constraint,
            expires_at: join_link.expires_at,
            max_uses: join_link.max_uses,
            used_count: join_link.used_count,
            created_by_user_id: join_link.created_by_user_id,
            revoked_at: join_link.revoked_at,
            created_at: join_link.created_at,
        }
    }
}

impl From<JoinLink> for JoinLinkResponse {
    fn from(join_link: JoinLink) -> Self {
        Self::from_join_link(join_link)
    }
}
