use super::*;
use crate::repositories::test_helpers::{insert_command_fixture, insert_printer_fixture};
use axum::{
    body::Body,
    http::{Method, Request, header::AUTHORIZATION},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

mod agents;
mod jobs;
mod printers;

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

#[tokio::test]
async fn health_check_reports_ok() {
    let (status, body) = request(app().await, Method::GET, "/healthz", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "status": "ok" }));
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
