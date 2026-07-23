use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use super::sqlite_database;
use crate::{
    db::Database,
    repositories::{
        AuditActor, AuditEventRepository, AuthRepository, TenantRepository, TenantTokenScope,
    },
};

#[tokio::test]
async fn sqlite_plugin_session_self_revoke_repository_contract() {
    exercise_plugin_session_self_revoke(sqlite_database().await).await;
}

#[tokio::test]
async fn postgres_plugin_session_self_revoke_repository_contract_when_configured() {
    let Some((database, admin, schema)) = isolated_postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    database.migrate().await.unwrap();

    exercise_plugin_session_self_revoke(database.clone()).await;

    let Database::Postgres(pool) = database else {
        panic!("expected PostgreSQL database");
    };
    pool.close().await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

async fn exercise_plugin_session_self_revoke(database: Database) {
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let audit = AuditEventRepository::new(database);
    let tenant = tenants
        .create("plugin-session-repository", "Plugin Session Repository")
        .await
        .unwrap();
    let studio = auth
        .create_tenant_token_with_audit(
            tenant.id,
            "Studio",
            vec![TenantTokenScope::PluginStudio],
            None,
            AuditActor::no_auth(),
        )
        .await
        .unwrap();
    let wrong_scope = auth
        .create_tenant_token_with_audit(
            tenant.id,
            "Admin",
            vec![TenantTokenScope::All],
            None,
            AuditActor::no_auth(),
        )
        .await
        .unwrap();
    let expired = auth
        .create_tenant_token_with_audit(
            tenant.id,
            "Expired Studio",
            vec![TenantTokenScope::PluginStudio],
            Some("2000-01-01T00:00:00Z".to_owned()),
            AuditActor::no_auth(),
        )
        .await
        .unwrap();

    assert!(
        auth.revoke_plugin_studio_token_with_audit("unknown-token")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        auth.revoke_plugin_studio_token_with_audit(&wrong_scope.plaintext_token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        auth.authenticate_tenant_token(&wrong_scope.plaintext_token)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        auth.authenticate_tenant_token(&expired.plaintext_token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        auth.revoke_plugin_studio_token_with_audit(&expired.plaintext_token)
            .await
            .unwrap()
            .unwrap()
            .revoked_at
            .is_some()
    );

    let (first, second) = tokio::join!(
        auth.revoke_plugin_studio_token_with_audit(&studio.plaintext_token),
        auth.revoke_plugin_studio_token_with_audit(&studio.plaintext_token),
    );
    assert_eq!(first.unwrap().unwrap().id, studio.token.id);
    assert_eq!(second.unwrap().unwrap().id, studio.token.id);
    assert!(
        auth.authenticate_tenant_token(&studio.plaintext_token)
            .await
            .unwrap()
            .is_none()
    );

    let revoke_events = audit
        .list_for_tenant(tenant.id)
        .await
        .unwrap()
        .into_iter()
        .filter(|event| event.action == "tenant_token.revoke")
        .collect::<Vec<_>>();
    assert_eq!(revoke_events.len(), 2);
    let studio_revoke = revoke_events
        .iter()
        .find(|event| event.target_id.as_deref() == Some(studio.token.id.as_str()))
        .unwrap();
    assert_eq!(studio_revoke.actor_type, "plugin_token");
    assert_eq!(
        studio_revoke.target_id.as_deref(),
        Some(studio.token.id.as_str())
    );
    assert_eq!(
        revoke_events
            .iter()
            .filter(|event| event.target_id.as_deref() == Some(studio.token.id.as_str()))
            .count(),
        1
    );
}

async fn isolated_postgres_database() -> Option<(Database, sqlx::PgPool, String)> {
    let url = std::env::var("PANDAR_TEST_POSTGRES_URL").ok()?;
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let schema = format!("pandar_plugin_revoke_{}", uuid::Uuid::new_v4().simple());
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

    Some((Database::Postgres(pool), admin, schema))
}
