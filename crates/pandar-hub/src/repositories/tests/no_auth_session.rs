use std::{str::FromStr, sync::OnceLock, time::Duration};

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use crate::{
    db::Database,
    repositories::{NoAuthPluginSessionOutcome, TenantTokenScope},
};

#[tokio::test]
async fn sqlite_no_auth_session_cardinality_and_creation_share_one_write_boundary() {
    exercise_atomic_cardinality(
        crate::AppState::file_sqlite_for_tests().await.unwrap(),
        "sqlite",
    )
    .await;
}

#[tokio::test]
async fn postgres_no_auth_session_cardinality_and_creation_share_one_lock_when_configured() {
    let Some((state, pool, admin, schema, spool_dir)) = isolated_postgres_state().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    exercise_atomic_cardinality(state, "postgres").await;

    drop(spool_dir);
    pool.close().await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

async fn exercise_atomic_cardinality(state: crate::AppState, slug_suffix: &str) {
    let _guard = race_test_lock().lock().await;
    let first = state
        .tenants()
        .create(
            format!("no-auth-atomic-first-{slug_suffix}"),
            "No Auth Atomic First",
        )
        .await
        .unwrap();
    let session_name = format!("Local Bambu Studio Plugin {slug_suffix}");
    let mut pause = crate::repositories::no_auth_session_test_pause::install(session_name.clone());
    let auth = state.auth().clone();
    let session = tokio::spawn(async move {
        auth.create_no_auth_plugin_session_with_audit(
            session_name,
            "2099-01-01T00:00:00Z".to_owned(),
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(5), pause.wait_until_counted())
        .await
        .expect("no-auth session must reach its post-cardinality pause");

    let tenants = state.tenants().clone();
    let second_slug = format!("no-auth-atomic-second-{slug_suffix}");
    let (insert_started, insert_start) = tokio::sync::oneshot::channel();
    let mut competing_insert = tokio::spawn(async move {
        let _ = insert_started.send(());
        tenants.create(second_slug, "No Auth Atomic Second").await
    });
    tokio::time::timeout(Duration::from_secs(5), insert_start)
        .await
        .expect("competing tenant insert task must start")
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut competing_insert)
            .await
            .is_err(),
        "tenant insert must wait for the no-auth session transaction"
    );

    pause.release();
    let outcome = session.await.unwrap().unwrap();
    let NoAuthPluginSessionOutcome::Created(session) = outcome else {
        panic!("single-tenant no-auth session should be created");
    };
    let session = *session;
    let tenant = session.tenant;
    let tenant_token = session.tenant_token;
    assert_eq!(tenant.id, first.id);
    assert_eq!(tenant_token.token.tenant_id, first.id);
    assert_eq!(tenant_token.token.scopes, [TenantTokenScope::PluginStudio]);

    let second = competing_insert.await.unwrap().unwrap();
    assert_eq!(
        state
            .auth()
            .list_tenant_tokens(first.id)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        state
            .auth()
            .list_tenant_tokens(second.id)
            .await
            .unwrap()
            .is_empty()
    );
    let first_events = state
        .audit_events()
        .list_for_tenant(first.id)
        .await
        .unwrap();
    assert_eq!(
        first_events
            .iter()
            .filter(|event| event.action == "tenant_token.create")
            .count(),
        1
    );
    assert!(
        !first_events
            .iter()
            .any(|event| event.metadata_json.contains(&tenant_token.plaintext_token))
    );
    assert!(
        state
            .audit_events()
            .list_for_tenant(second.id)
            .await
            .unwrap()
            .is_empty()
    );
}

fn race_test_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

async fn isolated_postgres_state() -> Option<(
    crate::AppState,
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
    let schema = format!("pandar_no_auth_atomic_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let options = PgConnectOptions::from_str(&url)
        .unwrap()
        .options([("search_path", schema.as_str())]);
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
        .unwrap();
    let database = Database::Postgres(pool.clone());
    database.migrate().await.unwrap();
    let spool_dir = tempfile::tempdir().unwrap();
    let artifact_storage = crate::artifacts::FilesystemArtifactStorage::new(
        spool_dir.path(),
        crate::artifacts::DEFAULT_MAX_ARTIFACT_BYTES,
    )
    .unwrap();
    let state = crate::AppState::from_database(database, artifact_storage)
        .await
        .unwrap();

    Some((state, pool, admin, schema, spool_dir))
}
