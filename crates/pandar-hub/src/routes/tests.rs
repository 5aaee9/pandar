use super::*;
use crate::repositories::test_helpers::insert_printer_fixture;
use axum::{
    body::Body,
    http::{Method, Request, header::AUTHORIZATION},
};
use http_body_util::BodyExt;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use tower::ServiceExt;

mod agent_printers;
mod agents;
mod artifacts;
mod auth_support;
mod basic;
mod bootstrap;
mod jobs;
mod multipart;
mod no_auth;
mod onboarding;
mod plugin;
mod plugin_multipart;
mod plugin_redaction;
mod printer_commands;
mod printer_events_ws;
mod printers;
mod provisioning;
mod readiness_metrics;
mod tenant_tokens;

use auth_support::*;
use multipart::{
    multipart_print_body, multipart_print_body_file_first, multipart_print_body_with_fields,
    multipart_print_body_with_mappings, multipart_request_as,
};

const TEST_PRIVATE_KEY_PEM: &str = include_str!("tests/fixtures/external_auth_private.pem");
const TEST_PUBLIC_JWK_JSON: &str = include_str!("tests/fixtures/external_auth_jwks.json");
const TEST_ISSUER: &str = "https://identity.example.test";
const TEST_AUDIENCE: &str = "https://api.pandar.test";
const TEST_PROVIDER: &str = "clerk";
const TEST_BOOTSTRAP_TOKEN: &str = "test-bootstrap-token";

async fn state() -> AppState {
    raw_state().await.with_bootstrap_token(TEST_BOOTSTRAP_TOKEN)
}

async fn raw_state() -> AppState {
    AppState::sqlite_for_tests().await.unwrap()
}

fn sibling_state(state: &AppState) -> AppState {
    state.sibling_for_tests()
}

async fn start_control_plane(state: AppState) -> tokio::task::JoinHandle<()> {
    let (handle, ready) = crate::runtime::spawn_control_plane_ready(state);
    ready.await.unwrap().unwrap();
    handle
}

async fn bootstrap_state() -> AppState {
    state().await
}

async fn app() -> Router {
    bootstrap_app().await
}

async fn bootstrap_app() -> Router {
    router(bootstrap_state().await)
}

async fn bootstrap_disabled_app() -> Router {
    router(raw_state().await)
}

async fn request(
    app: Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    request_with_token(app, method, uri, body, None).await
}

async fn request_as(
    app: Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    request_with_token(app, method, uri, body, Some(token)).await
}

async fn request_with_token(
    app: Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let body = if let Some(body) = body {
        builder = builder.header("content-type", "application/json");
        Body::from(body.to_string())
    } else {
        Body::empty()
    };

    let response = app.oneshot(builder.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&body).unwrap();

    (status, body)
}

fn json_body(input: impl Serialize) -> Value {
    serde_json::to_value(input).unwrap()
}

fn decode_json<T: DeserializeOwned>(value: Value) -> T {
    let json = serde_json::to_string(&value).unwrap();
    serde_json::from_str(&json).unwrap()
}

async fn raw_request_as(
    app: Router,
    method: Method,
    uri: &str,
    token: &str,
) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method(method)
            .uri(uri)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap()
}

async fn create_tenant_for_test(app: Router) -> (StatusCode, Value) {
    request_as(
        app,
        Method::POST,
        "/api/v1/tenants",
        Some(json_body(TestCreateTenantRequest {
            slug: "acme",
            display_name: "Acme Labs",
        })),
        TEST_BOOTSTRAP_TOKEN,
    )
    .await
}

#[derive(Serialize)]
struct TestCreateTenantRequest<'a> {
    slug: &'a str,
    display_name: &'a str,
}

async fn tenant_and_agent(state: &AppState, app: Router) -> (Value, Value, String) {
    let (status, tenant) = create_tenant_for_test(app.clone()).await;
    assert_eq!(status, StatusCode::CREATED);
    let tenant_id = decode_json::<TestTenantResponse>(tenant.clone()).id;
    let token = auth_token_for_role(
        state,
        &tenant_id,
        crate::repositories::UserRole::TenantAdmin,
        "admin-token",
    )
    .await;
    let (status, agent) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        Some(json_body(TestCreateAgentRequest { name: "shop-agent" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    (tenant, agent, token)
}

#[derive(Serialize)]
struct TestCreateAgentRequest<'a> {
    name: &'a str,
}

#[derive(Debug, serde::Deserialize)]
struct TestTenantResponse {
    id: String,
}

async fn auth_token_for_role(
    state: &AppState,
    tenant_id: &str,
    role: crate::repositories::UserRole,
    token: &str,
) -> String {
    let tenant_id = TenantId::parse(tenant_id).unwrap();
    let scopes = match role {
        crate::repositories::UserRole::TenantAdmin | crate::repositories::UserRole::Operator => {
            vec![crate::repositories::TenantTokenScope::All]
        }
        crate::repositories::UserRole::Viewer => Vec::new(),
    };
    let user = state
        .auth()
        .create_user(
            tenant_id,
            format!("{token}@example.test"),
            "Tenant Token Creator",
            crate::repositories::UserRole::TenantAdmin,
        )
        .await
        .unwrap();
    let plaintext = format!("test_tenant_{}_{}", token, uuid::Uuid::new_v4().simple());
    crate::entities::tenant_tokens::ActiveModel {
        id: Set(uuid::Uuid::new_v4().to_string()),
        tenant_id: Set(tenant_id.to_string()),
        name: Set(token.to_owned()),
        token_hash: Set(crate::repositories::hash_token_for_test(&plaintext)),
        scopes_json: Set(tenant_token_scopes_json(&scopes)),
        created_by_user_id: Set(Some(user.id)),
        created_at: Set(pandar_core::created_at_now()),
        last_used_at: Set(None),
        expires_at: Set(None),
        revoked_at: Set(None),
    }
    .insert(&state.database().sea_orm_connection())
    .await
    .unwrap();
    plaintext
}

async fn tenant_token_for_scopes(
    state: &AppState,
    tenant_id: &str,
    name: &str,
    scopes: Vec<crate::repositories::TenantTokenScope>,
) -> String {
    let tenant_id = TenantId::parse(tenant_id).unwrap();
    let user = state
        .auth()
        .create_user(
            tenant_id,
            format!("{name}@example.test"),
            "Tenant Token Creator",
            crate::repositories::UserRole::TenantAdmin,
        )
        .await
        .unwrap();
    let plaintext = format!("test_tenant_{}_{}", name, uuid::Uuid::new_v4().simple());
    crate::entities::tenant_tokens::ActiveModel {
        id: Set(uuid::Uuid::new_v4().to_string()),
        tenant_id: Set(tenant_id.to_string()),
        name: Set(name.to_owned()),
        token_hash: Set(crate::repositories::hash_token_for_test(&plaintext)),
        scopes_json: Set(tenant_token_scopes_json(&scopes)),
        created_by_user_id: Set(Some(user.id)),
        created_at: Set(pandar_core::created_at_now()),
        last_used_at: Set(None),
        expires_at: Set(None),
        revoked_at: Set(None),
    }
    .insert(&state.database().sea_orm_connection())
    .await
    .unwrap();
    plaintext
}

fn tenant_token_scopes_json(scopes: &[crate::repositories::TenantTokenScope]) -> String {
    serde_json::to_string(
        &scopes
            .iter()
            .map(|scope| scope.as_str())
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

async fn read_only_tenant_token(state: &AppState, tenant_id: &str, name: &str) -> String {
    tenant_token_for_scopes(state, tenant_id, name, Vec::new()).await
}

async fn all_scope_tenant_token(state: &AppState, tenant_id: &str, name: &str) -> String {
    tenant_token_for_scopes(
        state,
        tenant_id,
        name,
        vec![crate::repositories::TenantTokenScope::All],
    )
    .await
}

async fn agent_register_tenant_token(state: &AppState, tenant_id: &str, name: &str) -> String {
    tenant_token_for_scopes(
        state,
        tenant_id,
        name,
        vec![crate::repositories::TenantTokenScope::AgentRegister],
    )
    .await
}

async fn plugin_studio_tenant_token(state: &AppState, tenant_id: &str, name: &str) -> String {
    tenant_token_for_scopes(
        state,
        tenant_id,
        name,
        vec![crate::repositories::TenantTokenScope::PluginStudio],
    )
    .await
}

async fn all_and_plugin_studio_tenant_token(
    state: &AppState,
    tenant_id: &str,
    name: &str,
) -> String {
    tenant_token_for_scopes(
        state,
        tenant_id,
        name,
        vec![
            crate::repositories::TenantTokenScope::All,
            crate::repositories::TenantTokenScope::PluginStudio,
        ],
    )
    .await
}
