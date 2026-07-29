use super::*;
use requests::{
    agent_name_value, retired_api_token_value, tenant_token_create_value, user_role_value,
};
use serde::{Deserialize, de::DeserializeOwned};

mod requests;

#[derive(Debug, Deserialize)]
struct UserResponse {
    role: String,
}

#[derive(Debug, Deserialize)]
struct UserListResponse {
    users: Vec<UserResponse>,
    identities: Vec<UserIdentityResponse>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
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

fn decode<T>(body: Value) -> T
where
    T: DeserializeOwned,
{
    decode_json(body)
}

#[tokio::test]
async fn manual_user_provisioning_write_routes_are_not_available() {
    let (_state, app, tenant_id, admin_token) = admin_tenant().await;
    let user_id = uuid::Uuid::new_v4();

    for uri in [
        format!("/api/v1/tenants/{tenant_id}/users"),
        format!("/api/v1/tenants/{tenant_id}/users/{user_id}/identities"),
    ] {
        let response = raw_request_as(app.clone(), Method::POST, &uri, &admin_token).await;
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }
}

#[tokio::test]
async fn tenant_admin_can_list_users_identities_and_manage_roles_and_tokens() {
    let (state, app, tenant_id, admin_token) = admin_tenant().await;
    let tenant = state.tenants().list().await.unwrap().remove(0);
    let user = state
        .auth()
        .create_user(
            tenant.id,
            "operator@example.test",
            "Operator",
            crate::repositories::UserRole::Operator,
        )
        .await
        .unwrap();
    let identity = state
        .auth()
        .link_external_identity(tenant.id, &user.id, "clerk", "user_123")
        .await
        .unwrap();
    let user_id = user.id;
    let identity = UserIdentityResponse {
        provider: identity.provider,
        subject: identity.subject,
    };

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
    assert_eq!(users.identities, vec![identity.clone()]);

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
    assert_eq!(identities.identities, vec![identity.clone()]);

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
    assert_eq!(users.identities, vec![identity]);

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
            "user.role_update",
            "tenant_token.create",
            "tenant_token.revoke"
        ]
    );
    let role_metadata =
        serde_json::from_str::<UserRoleAuditMetadata>(&events[0].metadata_json).unwrap();
    assert_eq!(role_metadata.previous_role, "operator");
    assert_eq!(role_metadata.new_role, "viewer");
    assert!(!role_metadata.tenant_token_id.is_empty());
    assert_eq!(role_metadata.tenant_token_scopes, vec!["*".to_owned()]);
    let (status, body) = request_as(
        app,
        Method::PATCH,
        &format!("/api/v1/tenants/{tenant_id}/users/{user_id}/role"),
        Some(user_role_value("admin")),
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
            format!("/api/v1/tenants/{tenant_id}/users/{}/role", user.id),
            user_role_value(""),
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
