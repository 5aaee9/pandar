use crate::repositories::test_helpers::{insert_command_fixture, insert_printer_fixture};
use axum::{
    body::Body,
    http::{Method, Request, header::AUTHORIZATION},
};
use http_body_util::BodyExt;
use pandar_core::TenantId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tower::ServiceExt;

use super::*;

#[derive(Debug, Deserialize, PartialEq)]
struct SummaryResponse {
    tenants: i64,
    agents: i64,
    printers: i64,
    commands: i64,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TenantResponse {
    id: String,
    slug: String,
    display_name: String,
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct TenantsResponse {
    tenants: Vec<TenantResponse>,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Serialize)]
struct CreateAgentRequest<'a> {
    name: &'a str,
}

#[derive(Serialize)]
struct CreateTenantRequest<'a> {
    slug: &'a str,
    display_name: &'a str,
}

#[derive(Serialize)]
struct BootstrapTenantAdminRequest<'a> {
    tenant_slug: &'a str,
    tenant_display_name: &'a str,
    admin_email: &'a str,
    admin_display_name: &'a str,
    api_token_name: &'a str,
}

#[derive(Debug, Deserialize)]
struct BootstrapTenantAdminResponse {
    tenant: TenantResponse,
    user: BootstrapUserResponse,
    tenant_token: BootstrapTenantTokenResponse,
}

#[derive(Debug, Deserialize)]
struct BootstrapUserResponse {
    role: String,
}

#[derive(Debug, Deserialize)]
struct BootstrapTenantTokenResponse {
    name: String,
    scopes: Vec<String>,
    revoked_at: Option<String>,
    token: String,
}

#[derive(Debug, Deserialize)]
struct AgentsResponse {
    agents: Vec<AgentResponse>,
}

#[derive(Debug, Deserialize)]
struct AgentResponse {}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> T {
    decode_json(value)
}

#[tokio::test]
async fn summary_reports_repository_counts() {
    let state = bootstrap_state().await;
    let app = router(state.clone());
    let (status, _) = create_tenant_for_test(app.clone()).await;
    assert_eq!(status, StatusCode::CREATED);

    let tenants: TenantsResponse = decode(bootstrap_get(app.clone(), "/api/v1/tenants").await.1);
    let tenant_id = tenants.tenants[0].id.clone();
    let token = auth_token_for_role(&state, &tenant_id, admin(), "summary-admin").await;
    let (status, _) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        Some(serde_json::to_value(CreateAgentRequest { name: "shop-agent" }).unwrap()),
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

    let (status, body) = bootstrap_get(app, "/api/v1/summary").await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<SummaryResponse>(body);
    assert_eq!(
        body,
        SummaryResponse {
            tenants: 1,
            agents: 1,
            printers: 1,
            commands: 1
        }
    );
}

#[tokio::test]
async fn tenant_create_returns_created_record() {
    let (status, body) = create_tenant_for_test(bootstrap_app().await).await;

    assert_eq!(status, StatusCode::CREATED);
    let body = decode::<TenantResponse>(body);
    assert_eq!(body.slug, "acme");
    assert_eq!(body.display_name, "Acme Labs");
    assert!(!body.id.is_empty());
    assert!(body.created_at.ends_with('Z'));
}

#[tokio::test]
async fn tenant_list_returns_created_records() {
    let app = bootstrap_app().await;
    let (status, created) = create_tenant_for_test(app.clone()).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = bootstrap_get(app, "/api/v1/tenants").await;

    assert_eq!(status, StatusCode::OK);
    let created = decode::<TenantResponse>(created);
    let body = decode::<TenantsResponse>(body);
    assert_eq!(body.tenants, vec![created]);
}

#[tokio::test]
async fn duplicate_tenant_slug_returns_conflict() {
    let app = bootstrap_app().await;
    let (status, _) = create_tenant_for_test(app.clone()).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = create_tenant_for_test(app).await;

    assert_eq!(status, StatusCode::CONFLICT);
    let body = decode::<ErrorResponse>(body);
    assert_eq!(body.error, "tenant_slug_exists");
}

#[tokio::test]
async fn empty_tenant_fields_return_bad_request() {
    let (status, body) = bootstrap_post(
        bootstrap_app().await,
        "/api/v1/tenants",
        serde_json::to_value(CreateTenantRequest {
            slug: "",
            display_name: "Acme Labs",
        })
        .unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let body = decode::<ErrorResponse>(body);
    assert_eq!(body.error, "bad_request");
}

#[tokio::test]
async fn malformed_tenant_json_returns_bad_request() {
    let response = bootstrap_app()
        .await
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/tenants")
                .header("content-type", "application/json")
                .header(AUTHORIZATION, format!("Bearer {TEST_BOOTSTRAP_TOKEN}"))
                .body(Body::from("{"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: ErrorResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(body.error, "bad_request");
}

#[tokio::test]
async fn summary_and_tenant_listing_require_bootstrap_token() {
    let state = bootstrap_state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let tenant_token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        admin(),
        "tenant-admin-token",
    )
    .await;

    for uri in ["/api/v1/summary", "/api/v1/tenants"] {
        let (status, body) = request(app.clone(), Method::GET, uri, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let body = decode::<ErrorResponse>(body);
        assert_eq!(body.error, "missing_auth_token");

        let (status, body) = request_as(app.clone(), Method::GET, uri, None, "wrong-token").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let body = decode::<ErrorResponse>(body);
        assert_eq!(body.error, "invalid_auth_token");

        let (status, body) = request_as(app.clone(), Method::GET, uri, None, &tenant_token).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let body = decode::<ErrorResponse>(body);
        assert_eq!(body.error, "invalid_auth_token");

        let (status, _) = bootstrap_get(app.clone(), uri).await;
        assert_eq!(status, StatusCode::OK);
    }
}

#[tokio::test]
async fn bootstrap_disabled_rejects_bootstrap_only_endpoints() {
    let (status, body) = request_as(
        bootstrap_disabled_app().await,
        Method::GET,
        "/api/v1/summary",
        None,
        "any-token",
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    let body = decode::<ErrorResponse>(body);
    assert_eq!(body.error, "bootstrap_disabled");
}

#[tokio::test]
async fn bootstrap_tenant_admin_creates_tenant_user_token_and_audit_events() {
    let state = bootstrap_state().await;
    let app = router(state.clone());

    let (status, body) = bootstrap_post(
        app.clone(),
        "/api/v1/bootstrap/tenant-admin",
        serde_json::to_value(BootstrapTenantAdminRequest {
            tenant_slug: "bootstrap-acme",
            tenant_display_name: "Bootstrap Acme",
            admin_email: "admin@example.test",
            admin_display_name: "Admin",
            api_token_name: "bootstrap-admin",
        })
        .unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let body = decode::<BootstrapTenantAdminResponse>(body);
    assert_eq!(body.tenant.slug, "bootstrap-acme");
    assert_eq!(body.user.role, "tenant_admin");
    assert_eq!(body.tenant_token.name, "bootstrap-admin");
    assert_eq!(body.tenant_token.scopes, vec!["*".to_owned()]);
    assert_eq!(body.tenant_token.revoked_at, None);
    assert!(body.tenant_token.token.starts_with("pandar_tenant_"));

    let tenant_id = body.tenant.id.clone();
    let token = body.tenant_token.token.clone();
    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let body = decode::<AgentsResponse>(body);
    assert_eq!(body.agents.len(), 0);

    let events = state
        .audit_events()
        .list_for_tenant(TenantId::parse(&tenant_id).unwrap())
        .await
        .unwrap();
    let actions = events
        .iter()
        .map(|event| (event.actor_type.as_str(), event.action.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        actions,
        vec![
            ("bootstrap", "tenant.bootstrap"),
            ("bootstrap", "user.create"),
            ("bootstrap", "tenant_token.create")
        ]
    );
}

#[tokio::test]
async fn bootstrap_tenant_admin_rolls_back_on_late_failure() {
    let state = state().await;
    let tenant = state
        .tenants()
        .create("existing", "Existing")
        .await
        .unwrap();
    let user = state
        .auth()
        .create_user(
            tenant.id,
            "existing@example.test",
            "Existing Admin",
            admin(),
        )
        .await
        .unwrap();
    let before = rollback_counts(&state, tenant.id, &user.id).await;
    let err = duplicate_slug_bootstrap(&state, "existing").await;
    assert!(matches!(
        err,
        crate::repositories::RepositoryError::DuplicateTenantSlug
    ));
    assert_eq!(rollback_counts(&state, tenant.id, &user.id).await, before);
    assert_no_tenant_slug(&state, "rolled-back").await;
}

#[tokio::test]
async fn postgres_bootstrap_tenant_admin_transaction_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let state =
        AppState::from_database(database, crate::jobs::JobStorageConfig::from_env().unwrap());

    let bootstrapped = state
        .auth()
        .bootstrap_tenant_admin_with_plaintext_token(
            "postgres-bootstrap",
            "Postgres Bootstrap",
            "admin@postgres.test",
            "Admin",
            "bootstrap-admin",
        )
        .await
        .unwrap();
    assert_eq!(bootstrapped.tenant.slug, "postgres-bootstrap");
    assert_eq!(bootstrapped.user.role.as_str(), "tenant_admin");
    assert_eq!(bootstrapped.tenant_token.revoked_at, None);
    assert!(bootstrapped.plaintext_token.starts_with("pandar_tenant_"));
    assert_eq!(
        state
            .audit_events()
            .list_for_tenant(bootstrapped.tenant.id)
            .await
            .unwrap()
            .len(),
        3
    );

    let before_tenants = state.tenants().count().await.unwrap();
    let err = duplicate_slug_bootstrap(&state, "postgres-bootstrap").await;
    assert!(matches!(
        err,
        crate::repositories::RepositoryError::DuplicateTenantSlug
    ));
    assert_eq!(state.tenants().count().await.unwrap(), before_tenants);
    assert_no_tenant_slug(&state, "postgres-rolled-back").await;
}

async fn postgres_database() -> Option<crate::db::Database> {
    let url = match std::env::var("PANDAR_TEST_POSTGRES_URL") {
        Ok(url) => url,
        Err(_) => return None,
    };
    let config = crate::db::DatabaseConfig::from_url(url).unwrap();
    let database = crate::db::Database::connect(&config).await.unwrap();
    database.migrate().await.unwrap();
    let crate::db::Database::Postgres(pool) = &database else {
        panic!("expected PostgreSQL database");
    };
    sqlx::query(
        "TRUNCATE printer_event_tickets, audit_events, api_tokens, user_identities, join_links, tenant_tokens, plugin_login_tickets, job_filament_usages, printer_material_snapshots, machine_events, jobs, job_artifacts, commands, printers, agents, users, tenants",
    )
    .execute(pool)
    .await
    .unwrap();
    Some(database)
}

fn admin() -> crate::repositories::UserRole {
    crate::repositories::UserRole::TenantAdmin
}

async fn bootstrap_get(app: Router, uri: &str) -> (StatusCode, Value) {
    request_as(app, Method::GET, uri, None, TEST_BOOTSTRAP_TOKEN).await
}

async fn bootstrap_post(app: Router, uri: &str, body: Value) -> (StatusCode, Value) {
    request_as(app, Method::POST, uri, Some(body), TEST_BOOTSTRAP_TOKEN).await
}

async fn assert_no_tenant_slug(state: &AppState, slug: &str) {
    assert!(
        state
            .tenants()
            .list()
            .await
            .unwrap()
            .into_iter()
            .all(|tenant| tenant.slug != slug)
    );
}

async fn duplicate_slug_bootstrap(
    state: &AppState,
    tenant_slug: &str,
) -> crate::repositories::RepositoryError {
    state
        .auth()
        .bootstrap_tenant_admin_with_plaintext_token(
            tenant_slug,
            "Rolled Back",
            "admin@rolled-back.test",
            "Admin",
            "bootstrap-admin",
        )
        .await
        .unwrap_err()
}

async fn rollback_counts(
    state: &AppState,
    tenant_id: TenantId,
    user_id: &str,
) -> (i64, usize, usize, usize) {
    (
        state.tenants().count().await.unwrap(),
        state
            .auth()
            .list_users_for_tenant(tenant_id)
            .await
            .unwrap()
            .len(),
        state
            .auth()
            .list_api_tokens_for_user(tenant_id, user_id)
            .await
            .unwrap()
            .len(),
        state
            .audit_events()
            .list_for_tenant(tenant_id)
            .await
            .unwrap()
            .len(),
    )
}
