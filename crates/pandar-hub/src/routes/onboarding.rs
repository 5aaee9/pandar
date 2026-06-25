use axum::{
    Json,
    extract::State,
    extract::rejection::JsonRejection,
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    repositories::ExternalMembership,
    routes::{ApiError, TenantResponse, auth},
};

#[derive(Debug, Serialize)]
pub(super) struct MeResponse {
    identity: ExternalIdentityResponse,
    tenants: Vec<ExternalMembershipResponse>,
    can_self_create_tenant: bool,
}

#[derive(Debug, Serialize)]
struct ExternalIdentityResponse {
    provider: String,
    subject: String,
    email: Option<String>,
    email_verified: Option<bool>,
    display_name: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ExternalMembershipResponse {
    tenant_id: String,
    tenant_slug: String,
    display_name: String,
    role: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct AcceptedMembershipResponse {
    user_id: String,
    role: &'static str,
    created: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct CreatedTenantResponse {
    tenant: TenantResponse,
    membership: ExternalMembershipResponse,
}

#[derive(Debug, Serialize)]
pub(super) struct AcceptedJoinLinkResponse {
    tenant: TenantResponse,
    membership: AcceptedMembershipResponse,
    created: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateTenantRequest {
    slug: String,
    display_name: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct AcceptJoinLinkRequest {
    token: String,
}

pub(super) async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MeResponse>, ApiError> {
    let identity = auth::verify_external_identity(&state, &headers).await?;
    let tenants = state
        .auth()
        .list_external_memberships(&identity.provider, &identity.subject)
        .await?
        .into_iter()
        .map(ExternalMembershipResponse::from)
        .collect();

    let display_name = identity.display_name();
    Ok(Json(MeResponse {
        identity: ExternalIdentityResponse {
            provider: identity.provider,
            subject: identity.subject,
            email: identity.email,
            email_verified: identity.email_verified,
            display_name,
        },
        tenants,
        can_self_create_tenant: state.tenant_self_create_allowed(),
    }))
}

pub(super) async fn create_tenant(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<CreateTenantRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<CreatedTenantResponse>), ApiError> {
    if !state.tenant_self_create_allowed() {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "tenant_self_create_disabled",
        ));
    }
    let identity = auth::verify_external_identity(&state, &headers).await?;
    let profile = auth::external_profile(&identity)?;
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    if payload.slug.trim().is_empty() || payload.display_name.trim().is_empty() {
        return Err(ApiError::bad_request("bad_request"));
    }
    let membership = state
        .auth()
        .self_create_tenant_for_external_identity(payload.slug, payload.display_name, profile)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(CreatedTenantResponse {
            tenant: TenantResponse::from(membership.tenant.clone()),
            membership: ExternalMembershipResponse::from(membership),
        }),
    ))
}

pub(super) async fn accept_join_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<AcceptJoinLinkRequest>, JsonRejection>,
) -> Result<Json<AcceptedJoinLinkResponse>, ApiError> {
    let identity = auth::verify_external_identity(&state, &headers).await?;
    let profile = auth::external_profile(&identity)?;
    let Json(payload) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request"))?;
    if payload.token.trim().is_empty() {
        return Err(ApiError::bad_request("bad_request"));
    }
    let accepted = state
        .auth()
        .accept_join_link(&payload.token, profile)
        .await?;
    let membership = ExternalMembership {
        tenant: accepted.tenant.clone(),
        user: accepted.user.clone(),
    };

    Ok(Json(AcceptedJoinLinkResponse {
        tenant: TenantResponse::from(accepted.tenant),
        membership: AcceptedMembershipResponse::from_accept(membership, accepted.created),
        created: accepted.created,
    }))
}

impl From<ExternalMembership> for ExternalMembershipResponse {
    fn from(membership: ExternalMembership) -> Self {
        Self {
            tenant_id: membership.tenant.id.to_string(),
            tenant_slug: membership.tenant.slug,
            display_name: membership.tenant.display_name,
            role: membership.user.role.as_str(),
        }
    }
}

impl AcceptedMembershipResponse {
    fn from_accept(membership: ExternalMembership, created: bool) -> Self {
        Self {
            user_id: membership.user.id,
            role: membership.user.role.as_str(),
            created,
        }
    }
}
