use super::*;
use requests::{
    agent_name_value, retired_api_token_value, tenant_token_create_value, user_create_value,
    user_identity_value, user_role_value,
};
use serde::{Deserialize, de::DeserializeOwned};

mod requests;

#[derive(Debug, Deserialize)]
struct UserResponse {
    id: String,
    tenant_id: String,
    email: String,
    role: String,
}

#[derive(Debug, Deserialize)]
struct UserListResponse {
    users: Vec<UserResponse>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct UserIdentityResponse {
    provider: String,
    subject: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct UserIdentityListResponse {
    identities: Vec<UserIdentityResponse>,
}

#[derive(Debug, Deserialize)]
struct TenantTokenResponse {
    id: String,
    name: String,
    scopes: Vec<String>,
    revoked_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TenantTokenWithPlaintextResponse {
    tenant_token: TenantTokenResponse,
    token: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TenantTokenListResponse {
    tenant_tokens: Vec<TenantTokenResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevokeTenantTokenResponse {
    tenant_token: TenantTokenResponse,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize)]
struct UserRoleAuditMetadata {
    previous_role: String,
    new_role: String,
    tenant_token_id: String,
    tenant_token_scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UserIdentityAuditMetadata {
    provider: String,
    tenant_token_id: String,
    subject: Option<String>,
}

fn decode<T>(body: Value) -> T
where
    T: DeserializeOwned,
{
    decode_json(body)
}

#[tokio::test]
async fn tenant_admin_can_manage_users_identities_and_tokens() {
    let (state, app, tenant_id, admin_token) = admin_tenant().await;
    let tenant = state.tenants().list().await.unwrap().remove(0);

    let (status, user) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/users"),
        Some(user_create_value(
            "operator@example.test",
            "Operator",
            "operator",
        )),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let user = decode::<UserResponse>(user);
    assert_eq!(user.tenant_id, tenant_id);
    assert_eq!(user.email, "operator@example.test");
    assert_eq!(user.role, "operator");
    let user_id = user.id;

    let (status, users) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/users"),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let users = decode::<UserListResponse>(users);
    assert_eq!(users.users.len(), 2);

    let (status, updated) = request_as(
        app.clone(),
        Method::PATCH,
        &format!("/api/v1/tenants/{tenant_id}/users/{user_id}/role"),
        Some(user_role_value("viewer")),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let updated = decode::<UserResponse>(updated);
    assert_eq!(updated.role, "viewer");

    let (status, identity) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/users/{user_id}/identities"),
        Some(user_identity_value("clerk", "user_123")),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let identity = decode::<UserIdentityResponse>(identity);
    assert_eq!(identity.provider, "clerk");
    assert_eq!(identity.subject, "user_123");

    let (status, identities) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/users/{user_id}/identities"),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let identities = decode::<UserIdentityListResponse>(identities);
    assert_eq!(identities.identities, vec![identity]);

    let (status, token) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/tenant-tokens"),
        Some(tenant_token_create_value("automation", &["*"], None)),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let token = decode::<TenantTokenWithPlaintextResponse>(token);
    assert_eq!(token.tenant_token.name, "automation");
    assert_eq!(token.tenant_token.scopes, vec!["*".to_owned()]);
    assert!(token.token.starts_with("pandar_tenant_"));
    assert_eq!(token.tenant_token.revoked_at, None);
    let plaintext_token = token.token;
    let token_id = token.tenant_token.id;

    let (status, tokens) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/tenant-tokens"),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let tokens = decode::<TenantTokenListResponse>(tokens);
    assert_eq!(tokens.tenant_tokens.len(), 2);
    let listed = tokens
        .tenant_tokens
        .iter()
        .find(|token| token.id == token_id)
        .unwrap();
    assert_eq!(listed.revoked_at, None);

    let (status, _) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        None,
        &plaintext_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, revoked) = request_as(
        app.clone(),
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/tenant-tokens/{token_id}"),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let revoked = decode::<RevokeTenantTokenResponse>(revoked);
    assert_eq!(revoked.tenant_token.id, token_id);
    assert!(revoked.tenant_token.revoked_at.is_some());

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        None,
        &plaintext_token,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_auth_token");

    let events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    let actions = events
        .iter()
        .map(|event| event.action.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        actions,
        vec![
            "user.create",
            "user.role_update",
            "user_identity.link",
            "tenant_token.create",
            "tenant_token.revoke"
        ]
    );
    let role_metadata =
        serde_json::from_str::<UserRoleAuditMetadata>(&events[1].metadata_json).unwrap();
    assert_eq!(role_metadata.previous_role, "operator");
    assert_eq!(role_metadata.new_role, "viewer");
    assert!(!role_metadata.tenant_token_id.is_empty());
    assert_eq!(role_metadata.tenant_token_scopes, vec!["*".to_owned()]);
    let identity_metadata =
        serde_json::from_str::<UserIdentityAuditMetadata>(&events[2].metadata_json).unwrap();
    assert_eq!(identity_metadata.provider, "clerk");
    assert_eq!(identity_metadata.subject, None);
    assert!(!identity_metadata.tenant_token_id.is_empty());

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/users"),
        Some(user_create_value(
            "bad-role@example.test",
            "Bad Role",
            "admin",
        )),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_user_role");
}

#[tokio::test]
async fn provisioning_mutations_reject_empty_required_strings() {
    let (state, app, tenant_id, admin_token) = admin_tenant().await;
    let tenant = state.tenants().list().await.unwrap().remove(0);
    let user = state
        .auth()
        .create_user(
            tenant.id,
            "target@example.test",
            "Target User",
            crate::repositories::UserRole::Viewer,
        )
        .await
        .unwrap();

    for (uri, body) in [
        (
            format!("/api/v1/tenants/{tenant_id}/users"),
            user_create_value("", "Target", "viewer"),
        ),
        (
            format!("/api/v1/tenants/{tenant_id}/users"),
            user_create_value("empty-name@example.test", "", "viewer"),
        ),
        (
            format!("/api/v1/tenants/{tenant_id}/users"),
            user_create_value("empty-role@example.test", "Target", ""),
        ),
        (
            format!("/api/v1/tenants/{tenant_id}/users/{}/role", user.id),
            user_role_value(""),
        ),
        (
            format!("/api/v1/tenants/{tenant_id}/users/{}/identities", user.id),
            user_identity_value("", "subject"),
        ),
        (
            format!("/api/v1/tenants/{tenant_id}/users/{}/identities", user.id),
            user_identity_value("clerk", ""),
        ),
        (
            format!("/api/v1/tenants/{tenant_id}/agent-pairings"),
            agent_name_value(""),
        ),
    ] {
        let method = if uri.ends_with("/role") {
            Method::PATCH
        } else {
            Method::POST
        };
        let (status, body) = request_as(app.clone(), method, &uri, Some(body), &admin_token).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(decode::<ErrorResponse>(body).error, "bad_request");
    }

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/users/{}/api-tokens", user.id),
        Some(retired_api_token_value("")),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::GONE);
    assert_eq!(decode::<ErrorResponse>(body).error, "api_tokens_retired");
}
