use super::*;
use requests::{
    join_link_accept_body, join_link_create_body, join_link_create_with_email_body,
    join_link_create_with_email_constraint_body, tenant_create_body,
};
use serde::{Deserialize, de::DeserializeOwned};

mod requests;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct MeResponse {
    identity: ExternalIdentityResponse,
    tenants: Vec<ExternalMembershipResponse>,
    can_self_create_tenant: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ExternalIdentityResponse {
    provider: String,
    subject: String,
    email: Option<String>,
    email_verified: Option<bool>,
    display_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ExternalMembershipResponse {
    tenant_id: String,
    tenant_slug: String,
    display_name: String,
    role: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct TenantResponse {
    id: String,
    slug: String,
    display_name: String,
    created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreatedTenantResponse {
    tenant: TenantResponse,
    membership: ExternalMembershipResponse,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct AcceptedMembershipResponse {
    user_id: String,
    role: String,
    created: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptedJoinLinkResponse {
    tenant: TenantResponse,
    membership: AcceptedMembershipResponse,
    created: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct JoinLinkResponse {
    id: String,
    tenant_id: String,
    role: String,
    email_constraint: Option<String>,
    expires_at: String,
    max_uses: i32,
    used_count: i32,
    created_by_user_id: Option<String>,
    revoked_at: Option<String>,
    created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct JoinLinkWithPlaintextResponse {
    join_link: JoinLinkResponse,
    token: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct JoinLinkListResponse {
    join_links: Vec<JoinLinkResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ErrorResponse {
    error: String,
}

fn decode<T>(body: serde_json::Value) -> T
where
    T: DeserializeOwned,
{
    serde_json::from_value(body).unwrap()
}

#[tokio::test]
async fn me_returns_external_identity_and_memberships_without_side_effects() {
    let state = external_auth_state(state().await);
    let app = router(state.clone());
    let tenant = state.tenants().create("acme-me", "Acme Me").await.unwrap();
    let token = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Operator,
        "me-subject",
    )
    .await;

    let (status, body) = request_as(app, Method::GET, "/api/v1/me", None, &token).await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<MeResponse>(body);
    assert_eq!(body.identity.provider, TEST_PROVIDER);
    assert_eq!(body.identity.subject, "me-subject");
    assert_eq!(body.tenants.len(), 1);
    assert_eq!(body.tenants[0].tenant_id, tenant.id.to_string());
    assert_eq!(body.tenants[0].role, "operator");
}

#[tokio::test]
async fn me_succeeds_with_unverified_email_and_reports_onboarding_blocked() {
    let state = external_auth_state(state().await);
    let app = router(state);
    let token = jwt_for_profile("unverified", "unverified@example.test", false, "Unverified");

    let (status, body) = request_as(app, Method::GET, "/api/v1/me", None, &token).await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<MeResponse>(body);
    assert_eq!(
        body.identity.email.as_deref(),
        Some("unverified@example.test")
    );
    assert_eq!(body.identity.email_verified, Some(false));
    assert!(body.tenants.is_empty());
    assert!(body.can_self_create_tenant);
}

#[tokio::test]
async fn me_rejects_tenant_tokens() {
    let state = external_auth_state(state().await);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("acme-me-token", "Acme Me Token")
        .await
        .unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::TenantAdmin,
        "tenant-token",
    )
    .await;

    let (status, body) = request_as(app, Method::GET, "/api/v1/me", None, &token).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_auth_token");
}

#[tokio::test]
async fn self_create_tenant_creates_admin_projection() {
    let state = external_auth_state(state().await);
    let app = router(state.clone());
    let token = jwt_for_profile("creator", "creator@example.test", true, "Creator");

    let (status, body) = request_as(
        app,
        Method::POST,
        "/api/v1/onboarding/tenants",
        tenant_create_body("creator-lab", "Creator Lab"),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let body = decode::<CreatedTenantResponse>(body);
    assert_eq!(body.tenant.slug, "creator-lab");
    assert_eq!(body.membership.role, "tenant_admin");
    let tenant_id = TenantId::parse(&body.tenant.id).unwrap();
    assert!(
        state
            .auth()
            .authenticate_external_identity(tenant_id, TEST_PROVIDER, "creator")
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn self_create_tenant_can_be_disabled() {
    let state = external_auth_state(state().await).with_tenant_self_create_for_tests(false);
    let app = router(state);
    let token = jwt_for_profile("disabled", "disabled@example.test", true, "Disabled");

    let (status, body) = request_as(
        app,
        Method::POST,
        "/api/v1/onboarding/tenants",
        tenant_create_body("disabled-lab", "Disabled Lab"),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        decode::<ErrorResponse>(body).error,
        "tenant_self_create_disabled"
    );
}

#[tokio::test]
async fn self_create_tenant_allows_identity_with_existing_membership() {
    let state = external_auth_state(state().await);
    let app = router(state.clone());
    let existing = state
        .tenants()
        .create("existing-tenant", "Existing Tenant")
        .await
        .unwrap();
    let token = external_auth_token_for_role(
        &state,
        existing.id,
        crate::repositories::UserRole::Viewer,
        "multi-tenant-user",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        "/api/v1/onboarding/tenants",
        tenant_create_body("second-tenant", "Second Tenant"),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let body = decode::<CreatedTenantResponse>(body);
    assert_eq!(body.tenant.slug, "second-tenant");
    assert_eq!(body.membership.role, "tenant_admin");
}

#[tokio::test]
async fn tenant_admin_can_create_list_and_revoke_join_links() {
    let state = external_auth_state(state().await);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("join-admin", "Join Admin")
        .await
        .unwrap();
    let admin = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::TenantAdmin,
        "join-admin",
    )
    .await;

    let (status, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/join-links", tenant.id),
        join_link_create_with_email_body("operator", "member@example.test", 3600, 1),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let created = decode::<JoinLinkWithPlaintextResponse>(created);
    assert_eq!(created.join_link.role, "operator");
    assert_eq!(
        created.join_link.email_constraint.as_deref(),
        Some("member@example.test")
    );
    assert!(created.token.starts_with("pandar_join"));
    let join_link_id = created.join_link.id.clone();

    let (status, listed) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{}/join-links", tenant.id),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode::<JoinLinkListResponse>(listed).join_links.len(), 1);

    let (status, revoked) = request_as(
        app,
        Method::DELETE,
        &format!("/api/v1/tenants/{}/join-links/{join_link_id}", tenant.id),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(decode::<JoinLinkResponse>(revoked).revoked_at.is_some());
}

#[tokio::test]
async fn tenant_tokens_cannot_manage_join_links() {
    let state = external_auth_state(state().await);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("join-token-denied", "Join Token Denied")
        .await
        .unwrap();
    let token = all_scope_tenant_token(&state, &tenant.id.to_string(), "join-token-all").await;
    let admin = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::TenantAdmin,
        "join-token-admin",
    )
    .await;
    let (_, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/join-links", tenant.id),
        join_link_create_body("viewer"),
        &admin,
    )
    .await;
    let join_link_id = decode::<JoinLinkWithPlaintextResponse>(created)
        .join_link
        .id;

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/join-links", tenant.id),
        join_link_create_body("viewer"),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{}/join-links", tenant.id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");

    let (status, body) = request_as(
        app,
        Method::DELETE,
        &format!("/api/v1/tenants/{}/join-links/{join_link_id}", tenant.id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
}

#[tokio::test]
async fn join_link_accept_creates_member_from_body_token() {
    let state = external_auth_state(state().await);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("join-accept-route", "Join Accept Route")
        .await
        .unwrap();
    let admin = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::TenantAdmin,
        "join-accept-admin",
    )
    .await;
    let (_, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/join-links", tenant.id),
        join_link_create_body("viewer"),
        &admin,
    )
    .await;
    let token = decode::<JoinLinkWithPlaintextResponse>(created).token;
    let member = jwt_for_profile("join-member", "member@example.test", true, "Member");

    let (status, accepted) = request_as(
        app,
        Method::POST,
        "/api/v1/join-links/accept",
        join_link_accept_body(&token),
        &member,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let accepted = decode::<AcceptedJoinLinkResponse>(accepted);
    assert_eq!(accepted.tenant.id, tenant.id.to_string());
    assert_eq!(accepted.membership.role, "viewer");
    assert!(!accepted.membership.user_id.is_empty());
    assert!(accepted.membership.created);
    assert!(accepted.created);
}

#[tokio::test]
async fn join_link_accept_rejects_email_mismatch() {
    let state = external_auth_state(state().await);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("join-email-route", "Join Email Route")
        .await
        .unwrap();
    let admin = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::TenantAdmin,
        "join-email-admin",
    )
    .await;
    let (_, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/join-links", tenant.id),
        join_link_create_with_email_constraint_body("viewer", "allowed@example.test"),
        &admin,
    )
    .await;
    let created = decode::<JoinLinkWithPlaintextResponse>(created);
    let wrong = jwt_for_profile("wrong-email", "wrong@example.test", true, "Wrong");

    let (status, body) = request_as(
        app,
        Method::POST,
        "/api/v1/join-links/accept",
        join_link_accept_body(&created.token),
        &wrong,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        decode::<ErrorResponse>(body).error,
        "join_link_email_mismatch"
    );
}

#[tokio::test]
async fn join_link_accept_existing_member_keeps_role() {
    let state = external_auth_state(state().await);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("join-existing-route", "Join Existing Route")
        .await
        .unwrap();
    let admin = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::TenantAdmin,
        "existing-member",
    )
    .await;
    let (_, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/join-links", tenant.id),
        join_link_create_body("viewer"),
        &admin,
    )
    .await;
    let created = decode::<JoinLinkWithPlaintextResponse>(created);

    let (status, accepted) = request_as(
        app,
        Method::POST,
        "/api/v1/join-links/accept",
        join_link_accept_body(&created.token),
        &admin,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let accepted = decode::<AcceptedJoinLinkResponse>(accepted);
    assert_eq!(accepted.membership.role, "tenant_admin");
    assert!(!accepted.membership.created);
    assert!(!accepted.created);
}

#[tokio::test]
async fn join_link_audit_metadata_redacts_subject_and_secret() {
    let state = external_auth_state(state().await);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("join-audit-route", "Join Audit Route")
        .await
        .unwrap();
    let admin = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::TenantAdmin,
        "audit-admin",
    )
    .await;
    let (_, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/join-links", tenant.id),
        join_link_create_body("operator"),
        &admin,
    )
    .await;
    let token = decode::<JoinLinkWithPlaintextResponse>(created).token;
    let member = jwt_for_profile(
        "raw-route-subject-secret",
        "audit-member@example.test",
        true,
        "Audit Member",
    );
    let (status, _) = request_as(
        app,
        Method::POST,
        "/api/v1/join-links/accept",
        join_link_accept_body(&token),
        &member,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    let metadata = events
        .iter()
        .map(|event| event.metadata_json.as_str())
        .collect::<String>();
    assert!(!metadata.contains("raw-route-subject-secret"));
    assert!(!metadata.contains(&token));
    assert!(!metadata.contains("token_hash"));
}
