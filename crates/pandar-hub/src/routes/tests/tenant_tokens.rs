use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, de::DeserializeOwned};

use super::*;
use crate::entities::{audit_events, tenant_tokens};
use requests::{
    agent_name_body, retired_api_token_body, tenant_token_create_body,
    tenant_token_create_without_scopes_body, tenant_token_rotate_body, user_create_body,
};

mod requests;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct TenantTokenResponse {
    id: String,
    tenant_id: String,
    name: String,
    scopes: Vec<String>,
    created_by_user_id: Option<String>,
    created_at: String,
    last_used_at: Option<String>,
    expires_at: Option<String>,
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
struct RotatedTenantTokenResponse {
    tenant_token: TenantTokenResponse,
    token: String,
    rotated_from_token_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevokeTenantTokenResponse {
    tenant_token: TenantTokenResponse,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TenantTokenListResponse {
    tenant_tokens: Vec<TenantTokenResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentsResponse {
    agents: Vec<serde::de::IgnoredAny>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct UserResponse {
    id: String,
    tenant_id: String,
    email: String,
    display_name: String,
    role: String,
    created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UserCreateAuditMetadata {
    email: String,
    role: String,
    tenant_token_id: String,
    tenant_token_scopes: Vec<String>,
}

fn decode<T>(body: serde_json::Value) -> T
where
    T: DeserializeOwned,
{
    serde_json::from_value(body).unwrap()
}

#[tokio::test]
async fn tenant_token_routes_create_list_rotate_and_revoke_without_exposing_hashes() {
    let state = external_auth_state(state().await);
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let admin = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::TenantAdmin,
        "tenant-token-admin",
    )
    .await;

    let (status, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/tenant-tokens", tenant.id),
        tenant_token_create_body("Studio", &["*", "agent:register"], None),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let created = decode::<TenantTokenWithPlaintextResponse>(created);
    assert_eq!(created.tenant_token.name, "Studio");
    assert_eq!(
        created.tenant_token.scopes,
        vec!["*".to_owned(), "agent:register".to_owned()]
    );
    assert!(created.token.starts_with("pandar_tenant_"));
    let token_id = created.tenant_token.id.clone();

    let (status, listed) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{}/tenant-tokens", tenant.id),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let listed = decode::<TenantTokenListResponse>(listed);
    assert_eq!(listed.tenant_tokens.len(), 1);

    let (status, rotated) = request_as(
        app.clone(),
        Method::POST,
        &format!(
            "/api/v1/tenants/{}/tenant-tokens/{token_id}/rotate",
            tenant.id
        ),
        tenant_token_rotate_body(None),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let rotated = decode::<RotatedTenantTokenResponse>(rotated);
    assert_eq!(rotated.rotated_from_token_id, token_id);
    assert_ne!(rotated.tenant_token.id, token_id);
    assert!(rotated.token.starts_with("pandar_tenant_"));
    assert_ne!(rotated.token, created.token);

    let (status, revoked) = request_as(
        app,
        Method::DELETE,
        &format!("/api/v1/tenants/{}/tenant-tokens/{token_id}", tenant.id),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let revoked = decode::<RevokeTenantTokenResponse>(revoked);
    assert!(revoked.tenant_token.revoked_at.is_some());
}

#[tokio::test]
async fn tenant_token_scopes_enforce_read_all_agent_register_and_plugin_studio() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("scope", "Scope").await.unwrap();
    let tenant_id = tenant.id.to_string();
    let read_only = read_only_tenant_token(&state, &tenant_id, "read-only").await;
    let all = all_scope_tenant_token(&state, &tenant_id, "all").await;
    let agent_register = agent_register_tenant_token(&state, &tenant_id, "agent-register").await;
    let plugin_studio = plugin_studio_tenant_token(&state, &tenant_id, "plugin-studio").await;

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        None,
        &read_only,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(decode::<AgentsResponse>(body).agents.is_empty());

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        agent_name_body("read-only-agent"),
        &read_only,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        None,
        &agent_register,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");

    let (status, _) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        agent_name_body("agent-register-agent"),
        &agent_register,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agent-pairings"),
        agent_name_body("agent-register-agent"),
        &agent_register,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        agent_name_body("all-agent"),
        &all,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        None,
        &plugin_studio,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        agent_name_body("plugin-agent"),
        &plugin_studio,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
}

#[tokio::test]
async fn tenant_token_create_rejects_unknown_scope() {
    let state = external_auth_state(state().await);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("bad-scope", "Bad Scope")
        .await
        .unwrap();
    let admin = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::TenantAdmin,
        "tenant-token-bad-scope-admin",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{}/tenant-tokens", tenant.id),
        tenant_token_create_body("Bad", &["jobs:write"], None),
        &admin,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_scope");
}

#[tokio::test]
async fn tenant_token_create_requires_scopes_but_allows_explicit_empty_scopes() {
    let state = external_auth_state(state().await);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("required-scopes", "Required Scopes")
        .await
        .unwrap();
    let admin = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::TenantAdmin,
        "tenant-token-required-scopes-admin",
    )
    .await;

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/tenant-tokens", tenant.id),
        tenant_token_create_without_scopes_body("Missing scopes", None),
        &admin,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "bad_request");

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{}/tenant-tokens", tenant.id),
        tenant_token_create_body("Read only", &[], None),
        &admin,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let body = decode::<TenantTokenWithPlaintextResponse>(body);
    assert_eq!(body.tenant_token.scopes, Vec::<String>::new());
}

#[tokio::test]
async fn tenant_token_create_and_rotate_reject_invalid_expires_at() {
    let state = external_auth_state(state().await);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("bad-expiry", "Bad Expiry")
        .await
        .unwrap();
    let admin = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::TenantAdmin,
        "tenant-token-bad-expiry-admin",
    )
    .await;

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/tenant-tokens", tenant.id),
        tenant_token_create_body("Bad Expiry", &["*"], Some("not-rfc3339")),
        &admin,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_expires_at");

    let (status, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/tenant-tokens", tenant.id),
        tenant_token_create_body("Good Expiry", &["*"], Some("2027-01-01T00:00:00Z")),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let token_id = decode::<TenantTokenWithPlaintextResponse>(created)
        .tenant_token
        .id;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!(
            "/api/v1/tenants/{}/tenant-tokens/{token_id}/rotate",
            tenant.id
        ),
        tenant_token_rotate_body(Some("also-not-rfc3339")),
        &admin,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_expires_at");
}

#[tokio::test]
async fn all_scope_tenant_token_without_creator_can_perform_admin_mutation() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("token-admin", "Token Admin")
        .await
        .unwrap();
    let plaintext = format!("test_tenant_no_creator_{}", uuid::Uuid::new_v4().simple());
    tenant_tokens::ActiveModel {
        id: Set(uuid::Uuid::new_v4().to_string()),
        tenant_id: Set(tenant.id.to_string()),
        name: Set("no-creator-admin".to_owned()),
        token_hash: Set(crate::repositories::hash_token_for_test(&plaintext)),
        scopes_json: Set(tenant_token_scopes_json(&[
            crate::repositories::TenantTokenScope::All,
        ])),
        created_by_user_id: Set(None),
        created_at: Set(pandar_core::created_at_now()),
        last_used_at: Set(None),
        expires_at: Set(None),
        revoked_at: Set(None),
    }
    .insert(&state.database().sea_orm_connection())
    .await
    .unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{}/users", tenant.id),
        user_create_body(
            "created-by-token@example.test",
            "Created By Token",
            "viewer",
        ),
        &plaintext,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        decode::<UserResponse>(body).email,
        "created-by-token@example.test"
    );

    let event = audit_events::Entity::find()
        .filter(audit_events::Column::TenantId.eq(tenant.id.to_string()))
        .filter(audit_events::Column::Action.eq("user.create"))
        .one(&state.database().sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.actor_type, "tenant_token");
    assert_eq!(event.user_id, None);
}

#[tokio::test]
async fn tenant_token_audit_actor_preserves_creator_and_token_metadata() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("token-audit", "Token Audit")
        .await
        .unwrap();
    let user = state
        .auth()
        .create_user(
            tenant.id,
            "token-audit-admin@example.test",
            "Token Audit Admin",
            crate::repositories::UserRole::TenantAdmin,
        )
        .await
        .unwrap();
    let plaintext = format!("test_tenant_creator_{}", uuid::Uuid::new_v4().simple());
    let token_id = uuid::Uuid::new_v4().to_string();
    tenant_tokens::ActiveModel {
        id: Set(token_id.clone()),
        tenant_id: Set(tenant.id.to_string()),
        name: Set("creator-admin".to_owned()),
        token_hash: Set(crate::repositories::hash_token_for_test(&plaintext)),
        scopes_json: Set(tenant_token_scopes_json(&[
            crate::repositories::TenantTokenScope::All,
        ])),
        created_by_user_id: Set(Some(user.id.clone())),
        created_at: Set(pandar_core::created_at_now()),
        last_used_at: Set(None),
        expires_at: Set(None),
        revoked_at: Set(None),
    }
    .insert(&state.database().sea_orm_connection())
    .await
    .unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{}/users", tenant.id),
        user_create_body(
            "created-by-creator-token@example.test",
            "Created By Creator Token",
            "viewer",
        ),
        &plaintext,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        decode::<UserResponse>(body).email,
        "created-by-creator-token@example.test"
    );

    let event = audit_events::Entity::find()
        .filter(audit_events::Column::TenantId.eq(tenant.id.to_string()))
        .filter(audit_events::Column::Action.eq("user.create"))
        .one(&state.database().sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.actor_type, "tenant_token");
    assert_eq!(event.user_id, Some(user.id));
    let metadata = serde_json::from_str::<UserCreateAuditMetadata>(&event.metadata_json).unwrap();
    assert_eq!(metadata.tenant_token_id, token_id);
    assert_eq!(metadata.tenant_token_scopes, vec!["*".to_owned()]);
    assert_eq!(metadata.email, "created-by-creator-token@example.test");
    assert_eq!(metadata.role, "viewer");
}

#[tokio::test]
async fn expired_and_revoked_tenant_tokens_are_rejected() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("invalid", "Invalid").await.unwrap();
    let connection = state.database().sea_orm_connection();
    let expired = all_scope_tenant_token(&state, &tenant.id.to_string(), "expired").await;
    let expired_model = tenant_tokens::Entity::find()
        .filter(tenant_tokens::Column::Name.eq("expired"))
        .one(&connection)
        .await
        .unwrap()
        .unwrap();
    let mut active: tenant_tokens::ActiveModel = expired_model.into();
    active.expires_at = Set(Some("2000-01-01T00:00:00Z".to_owned()));
    active.update(&connection).await.unwrap();
    let revoked = all_scope_tenant_token(&state, &tenant.id.to_string(), "revoked").await;
    let stored = state.auth().list_tenant_tokens(tenant.id).await.unwrap();
    let revoked_id = stored
        .iter()
        .find(|token| token.name == "revoked")
        .unwrap()
        .id
        .clone();
    state
        .auth()
        .revoke_tenant_token_with_audit(
            tenant.id,
            &revoked_id,
            crate::repositories::AuditActor::tenant_token(None, revoked_id.clone(), vec!["*"]),
        )
        .await
        .unwrap();

    for token in [expired.as_str(), revoked.as_str()] {
        let (status, body) = request_as(
            app.clone(),
            Method::GET,
            &format!("/api/v1/tenants/{}/agents", tenant.id),
            None,
            token,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(decode::<ErrorResponse>(body).error, "invalid_auth_token");
    }
}

#[tokio::test]
async fn retired_api_token_routes_always_return_gone() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("retired", "Retired").await.unwrap();
    let user = state
        .auth()
        .create_user(
            tenant.id,
            "retired@example.test",
            "Retired",
            crate::repositories::UserRole::TenantAdmin,
        )
        .await
        .unwrap();
    let token = state
        .auth()
        .create_api_token(tenant.id, &user.id, "retired", "retired-secret")
        .await
        .unwrap();

    for (method, uri, bearer) in [
        (
            Method::GET,
            format!("/api/v1/tenants/{}/users/{}/api-tokens", tenant.id, user.id),
            None,
        ),
        (
            Method::POST,
            format!("/api/v1/tenants/{}/users/{}/api-tokens", tenant.id, user.id),
            Some("malformed"),
        ),
        (
            Method::DELETE,
            format!("/api/v1/tenants/{}/api-tokens/{}", tenant.id, token.id),
            Some("retired-secret"),
        ),
    ] {
        let (status, body) = request_with_token(
            app.clone(),
            method,
            &uri,
            retired_api_token_body("ignored"),
            bearer,
        )
        .await;
        assert_eq!(status, StatusCode::GONE);
        assert_eq!(decode::<ErrorResponse>(body).error, "api_tokens_retired");
    }
}
