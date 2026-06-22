use super::*;
use crate::repositories::test_helpers::{insert_command_fixture, insert_printer_fixture};
use axum::{
    body::Body,
    http::{Method, Request, header::AUTHORIZATION},
};
use http_body_util::BodyExt;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode, jwk::JwkSet};
use serde::Serialize;
use serde_json::{Value, json};
use tower::ServiceExt;

mod agents;
mod jobs;
mod printer_events_ws;
mod printers;

const TEST_PRIVATE_KEY_PEM: &str = include_str!("tests/fixtures/external_auth_private.pem");
const TEST_PUBLIC_JWK_JSON: &str = include_str!("tests/fixtures/external_auth_jwks.json");
const TEST_ISSUER: &str = "https://identity.example.test";
const TEST_AUDIENCE: &str = "https://api.pandar.test";
const TEST_PROVIDER: &str = "clerk";

async fn state() -> AppState {
    AppState::sqlite_for_tests().await.unwrap()
}

async fn app() -> Router {
    router(state().await)
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

async fn create_tenant_for_test(app: Router) -> (StatusCode, Value) {
    request(
        app,
        Method::POST,
        "/api/v1/tenants",
        Some(json!({
            "slug": "acme",
            "display_name": "Acme Labs"
        })),
    )
    .await
}

async fn tenant_and_agent(state: &AppState, app: Router) -> (Value, Value, String) {
    let (status, tenant) = create_tenant_for_test(app.clone()).await;
    assert_eq!(status, StatusCode::CREATED);
    let tenant_id = tenant["id"].as_str().unwrap();
    let token = auth_token_for_role(
        state,
        tenant_id,
        crate::repositories::UserRole::TenantAdmin,
        "admin-token",
    )
    .await;
    let (status, agent) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        Some(json!({ "name": "shop-agent" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    (tenant, agent, token)
}

async fn auth_token_for_role(
    state: &AppState,
    tenant_id: &str,
    role: crate::repositories::UserRole,
    token: &str,
) -> String {
    let tenant_id = TenantId::parse(tenant_id).unwrap();
    let user = state
        .auth()
        .create_user(
            tenant_id,
            format!("{token}@example.test"),
            "Test User",
            role,
        )
        .await
        .unwrap();
    state
        .auth()
        .create_api_token(tenant_id, &user.id, token, token)
        .await
        .unwrap();
    token.to_owned()
}

fn external_auth_state(state: AppState) -> AppState {
    let config = crate::identity::ExternalAuthConfig {
        provider: TEST_PROVIDER.to_owned(),
        issuer: TEST_ISSUER.to_owned(),
        jwks_url: "https://identity.example.test/.well-known/jwks.json".to_owned(),
        audience: Some(TEST_AUDIENCE.to_owned()),
        algorithms: vec![Algorithm::RS256],
        authorized_parties: Vec::new(),
        required_scopes: Vec::new(),
        leeway_seconds: 60,
    };
    let jwks = serde_json::from_str::<JwkSet>(TEST_PUBLIC_JWK_JSON).unwrap();
    state.with_external_auth(crate::identity::JwtVerifier::static_jwks(config, jwks))
}

fn jwt_for(
    subject: &str,
    issuer: &str,
    audience: &str,
    kid: &str,
    exp_offset_seconds: i64,
) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_owned());
    let now = jsonwebtoken::get_current_timestamp() as i64;
    let exp = now.saturating_add(exp_offset_seconds).max(0) as u64;
    let nbf = now.saturating_sub(30).max(0) as u64;
    encode(
        &header,
        &ExternalAuthClaims {
            iss: issuer,
            sub: subject,
            aud: audience,
            exp,
            nbf,
        },
        &EncodingKey::from_rsa_pem(TEST_PRIVATE_KEY_PEM.as_bytes()).unwrap(),
    )
    .unwrap()
}

async fn external_auth_token_for_role(
    state: &AppState,
    tenant_id: TenantId,
    role: crate::repositories::UserRole,
    subject: &str,
) -> String {
    let user = state
        .auth()
        .create_user(
            tenant_id,
            format!("{subject}@example.test"),
            "External Test User",
            role,
        )
        .await
        .unwrap();
    state
        .auth()
        .link_external_identity(tenant_id, &user.id, TEST_PROVIDER, subject)
        .await
        .unwrap();
    jwt_for(subject, TEST_ISSUER, TEST_AUDIENCE, "test-key", 3600)
}

#[derive(Serialize)]
struct ExternalAuthClaims<'a> {
    iss: &'a str,
    sub: &'a str,
    aud: &'a str,
    exp: u64,
    nbf: u64,
}

#[tokio::test]
async fn health_check_reports_ok() {
    let (status, body) = request(app().await, Method::GET, "/healthz", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "status": "ok" }));
}

#[tokio::test]
async fn api_token_auth_still_succeeds_when_external_auth_is_configured() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "api-token-with-external-auth",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{}/agents", tenant.id),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "agents": [] }));
}

#[tokio::test]
async fn summary_reports_repository_counts() {
    let state = state().await;
    let app = router(state.clone());
    let (status, _) = create_tenant_for_test(app.clone()).await;
    assert_eq!(status, StatusCode::CREATED);

    let tenant_id = request(app.clone(), Method::GET, "/api/v1/tenants", None)
        .await
        .1["tenants"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let token = auth_token_for_role(
        &state,
        &tenant_id,
        crate::repositories::UserRole::TenantAdmin,
        "summary-admin",
    )
    .await;
    let (status, _) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        Some(json!({ "name": "shop-agent" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let agents = state
        .agents()
        .list_for_tenant(TenantId::parse(&tenant_id).unwrap())
        .await
        .unwrap();
    let printer_id = insert_printer_fixture(state.database(), agents[0].tenant_id, agents[0].id)
        .await
        .unwrap();
    insert_command_fixture(
        state.database(),
        agents[0].tenant_id,
        agents[0].id,
        Some(&printer_id),
    )
    .await
    .unwrap();

    let (status, body) = request(app, Method::GET, "/api/v1/summary", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({ "tenants": 1, "agents": 1, "printers": 1, "commands": 1 })
    );
}

#[tokio::test]
async fn tenant_create_returns_created_record() {
    let (status, body) = create_tenant_for_test(app().await).await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["slug"], "acme");
    assert_eq!(body["display_name"], "Acme Labs");
    assert!(body["id"].as_str().is_some());
    assert!(body["created_at"].as_str().unwrap().ends_with('Z'));
}

#[tokio::test]
async fn tenant_list_returns_created_records() {
    let app = app().await;
    let (status, created) = create_tenant_for_test(app.clone()).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = request(app, Method::GET, "/api/v1/tenants", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "tenants": [created] }));
}

#[tokio::test]
async fn duplicate_tenant_slug_returns_conflict() {
    let app = app().await;
    let (status, _) = create_tenant_for_test(app.clone()).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = create_tenant_for_test(app).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body, json!({ "error": "tenant_slug_exists" }));
}

#[tokio::test]
async fn empty_tenant_fields_return_bad_request() {
    let (status, body) = request(
        app().await,
        Method::POST,
        "/api/v1/tenants",
        Some(json!({ "slug": "", "display_name": "Acme Labs" })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "bad_request" }));
}

#[tokio::test]
async fn malformed_tenant_json_returns_bad_request() {
    let response = app()
        .await
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/tenants")
                .header("content-type", "application/json")
                .body(Body::from("{"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body, json!({ "error": "bad_request" }));
}
