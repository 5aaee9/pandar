use super::*;
use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

#[tokio::test]
async fn plugin_no_auth_session_is_only_available_in_no_auth_mode() {
    let no_auth_state = state().await.with_no_auth_for_tests(true);
    let app = router(no_auth_state.clone());
    let tenant = no_auth_state
        .tenants()
        .create("plugin-no-auth", "Plugin No Auth")
        .await
        .unwrap();

    let (status, session) = request(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/no-auth-session",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let session = decode::<ExchangeLoginTicketResponse>(session);
    assert!(session.token.starts_with("pandar_tenant_"));
    assert_eq!(session.profile.tenant_id, tenant.id.to_string());
    assert_eq!(session.profile.tenant_name, "Plugin No Auth");

    let (status, body) = request_as(
        app,
        Method::GET,
        "/api/v1/plugin/printers",
        None,
        &session.token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let body = decode::<PluginPrinterListResponse>(body);
    assert_eq!(body.message, "success");
    assert!(body.devices.is_empty());

    let app = router(state().await);
    let (status, body) = request(app, Method::POST, "/api/v1/plugin/no-auth-session", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "no_auth_required");
}

#[tokio::test]
async fn plugin_no_auth_session_rejects_ambiguous_tenant_without_side_effects() {
    assert_ambiguous_no_auth_tenant_is_fail_closed(state().await.with_no_auth_for_tests(true))
        .await;
}

#[tokio::test]
async fn plugin_no_auth_session_requires_an_existing_tenant() {
    let state = state().await.with_no_auth_for_tests(true);

    let (status, body) = request(
        router(state),
        Method::POST,
        "/api/v1/plugin/no-auth-session",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(decode::<ErrorResponse>(body).error, "tenant_not_found");
}

#[tokio::test]
async fn plugin_no_auth_session_rejects_ambiguous_postgres_tenant_when_configured() {
    let Some((state, pool, admin, schema, spool_dir)) = isolated_postgres_no_auth_state().await
    else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    assert_ambiguous_no_auth_tenant_is_fail_closed(state).await;

    drop(spool_dir);
    pool.close().await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

async fn assert_ambiguous_no_auth_tenant_is_fail_closed(state: AppState) {
    let first = state
        .tenants()
        .create("plugin-no-auth-first", "Plugin No Auth First")
        .await
        .unwrap();
    let second = state
        .tenants()
        .create("plugin-no-auth-second", "Plugin No Auth Second")
        .await
        .unwrap();

    let (status, body) = request(
        router(state.clone()),
        Method::POST,
        "/api/v1/plugin/no-auth-session",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        decode::<ErrorResponse>(body).error,
        "ambiguous_no_auth_tenant"
    );
    for tenant in [first, second] {
        assert!(
            state
                .auth()
                .list_tenant_tokens(tenant.id)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            state
                .audit_events()
                .list_for_tenant(tenant.id)
                .await
                .unwrap()
                .is_empty()
        );
    }
}

async fn isolated_postgres_no_auth_state() -> Option<(
    AppState,
    sqlx::PgPool,
    sqlx::PgPool,
    String,
    tempfile::TempDir,
)> {
    let url = std::env::var("PANDAR_TEST_POSTGRES_URL").ok()?;
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let schema = format!("pandar_no_auth_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let options = PgConnectOptions::from_str(&url)
        .unwrap()
        .options([("search_path", schema.as_str())]);
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let database = crate::db::Database::Postgres(pool.clone());
    database.migrate().await.unwrap();
    let spool_dir = tempfile::tempdir().unwrap();
    let artifact_storage = crate::artifacts::FilesystemArtifactStorage::new(
        spool_dir.path(),
        crate::artifacts::DEFAULT_MAX_ARTIFACT_BYTES,
    )
    .unwrap();
    let state = AppState::from_database(database, artifact_storage)
        .await
        .unwrap()
        .with_no_auth_for_tests(true);

    Some((state, pool, admin, schema, spool_dir))
}

#[tokio::test]
async fn plugin_routes_only_accept_plugin_studio_tokens() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-auth", "Plugin Auth")
        .await
        .unwrap();
    let plugin = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "studio").await;
    let all = all_scope_tenant_token(&state, &tenant.id.to_string(), "all").await;
    let empty = read_only_tenant_token(&state, &tenant.id.to_string(), "empty").await;
    let mixed = all_and_plugin_studio_tenant_token(&state, &tenant.id.to_string(), "mixed").await;

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        "/api/v1/plugin/printers",
        None,
        &plugin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let body = decode::<PluginPrinterListResponse>(body);
    assert_eq!(body.message, "success");
    assert!(body.devices.is_empty());

    for denied in [&all, &empty, &mixed] {
        let (status, body) = request_as(
            app.clone(),
            Method::GET,
            "/api/v1/plugin/printers",
            None,
            denied,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
    }
}
