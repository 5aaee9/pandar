use crate::repositories::test_helpers::{insert_command_fixture, insert_printer_fixture};
use axum::{
    body::Body,
    http::{Method, Request, header::AUTHORIZATION},
};
use http_body_util::BodyExt;
use pandar_core::TenantId;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::*;

#[tokio::test]
async fn summary_reports_repository_counts() {
    let state = bootstrap_state().await;
    let app = router(state.clone());
    let (status, _) = create_tenant_for_test(app.clone()).await;
    assert_eq!(status, StatusCode::CREATED);

    let tenant_id = bootstrap_get(app.clone(), "/api/v1/tenants").await.1["tenants"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let token = auth_token_for_role(&state, &tenant_id, admin(), "summary-admin").await;
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

    let (status, body) = bootstrap_get(app, "/api/v1/summary").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({ "tenants": 1, "agents": 1, "printers": 1, "commands": 1 })
    );
}

#[tokio::test]
async fn tenant_create_returns_created_record() {
    let (status, body) = create_tenant_for_test(bootstrap_app().await).await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["slug"], "acme");
    assert_eq!(body["display_name"], "Acme Labs");
    assert!(body["id"].as_str().is_some());
    assert!(body["created_at"].as_str().unwrap().ends_with('Z'));
}

#[tokio::test]
async fn tenant_list_returns_created_records() {
    let app = bootstrap_app().await;
    let (status, created) = create_tenant_for_test(app.clone()).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = bootstrap_get(app, "/api/v1/tenants").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "tenants": [created] }));
}

#[tokio::test]
async fn duplicate_tenant_slug_returns_conflict() {
    let app = bootstrap_app().await;
    let (status, _) = create_tenant_for_test(app.clone()).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = create_tenant_for_test(app).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body, json!({ "error": "tenant_slug_exists" }));
}

#[tokio::test]
async fn empty_tenant_fields_return_bad_request() {
    let (status, body) = bootstrap_post(
        bootstrap_app().await,
        "/api/v1/tenants",
        json!({ "slug": "", "display_name": "Acme Labs" }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "bad_request" }));
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
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body, json!({ "error": "bad_request" }));
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
        assert_eq!(body, json!({ "error": "missing_auth_token" }));

        let (status, body) = request_as(app.clone(), Method::GET, uri, None, "wrong-token").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body, json!({ "error": "invalid_auth_token" }));

        let (status, body) = request_as(app.clone(), Method::GET, uri, None, &tenant_token).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body, json!({ "error": "invalid_auth_token" }));

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
    assert_eq!(body, json!({ "error": "bootstrap_disabled" }));
}

#[tokio::test]
async fn bootstrap_tenant_admin_creates_tenant_user_token_and_audit_events() {
    let state = bootstrap_state().await;
    let app = router(state.clone());

    let (status, body) = bootstrap_post(
        app.clone(),
        "/api/v1/bootstrap/tenant-admin",
        json!({
            "tenant_slug": "bootstrap-acme",
            "tenant_display_name": "Bootstrap Acme",
            "admin_email": "admin@example.test",
            "admin_display_name": "Admin",
            "api_token_name": "bootstrap-admin"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["tenant"]["slug"], "bootstrap-acme");
    assert_eq!(body["user"]["role"], "tenant_admin");
    assert_eq!(body["api_token"]["name"], "bootstrap-admin");
    assert_eq!(body["api_token"]["revoked_at"], Value::Null);
    let token = body["api_token"]["token"].as_str().unwrap();
    assert!(token.starts_with("pandar_"));

    let tenant_id = body["tenant"]["id"].as_str().unwrap();
    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        None,
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "agents": [] }));

    let events = state
        .audit_events()
        .list_for_tenant(TenantId::parse(tenant_id).unwrap())
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
            ("bootstrap", "api_token.create")
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
    state
        .auth()
        .create_api_token(
            tenant.id,
            &user.id,
            "existing-token",
            "fixed-bootstrap-secret",
        )
        .await
        .unwrap();

    let before = rollback_counts(&state, tenant.id, &user.id).await;
    let err = duplicate_hash_bootstrap(&state, "rolled-back", "fixed-bootstrap-secret").await;
    assert!(matches!(
        err,
        crate::repositories::RepositoryError::DuplicateApiTokenHash
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
            "postgres-bootstrap-secret",
        )
        .await
        .unwrap();
    assert_eq!(bootstrapped.tenant.slug, "postgres-bootstrap");
    assert_eq!(bootstrapped.user.role.as_str(), "tenant_admin");
    assert_eq!(bootstrapped.api_token.revoked_at, None);
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
    let err =
        duplicate_hash_bootstrap(&state, "postgres-rolled-back", "postgres-bootstrap-secret").await;
    assert!(matches!(
        err,
        crate::repositories::RepositoryError::DuplicateApiTokenHash
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
        "TRUNCATE audit_events, api_tokens, user_identities, jobs, job_artifacts, commands, printers, agents, users, tenants",
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

async fn duplicate_hash_bootstrap(
    state: &AppState,
    tenant_slug: &str,
    plaintext_token: &str,
) -> crate::repositories::RepositoryError {
    state
        .auth()
        .bootstrap_tenant_admin_with_plaintext_token(
            tenant_slug,
            "Rolled Back",
            "admin@rolled-back.test",
            "Admin",
            "bootstrap-admin",
            plaintext_token,
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
