use std::str::FromStr;

use serde::Serialize;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use super::*;
use crate::repositories::{
    AuditActor, ExternalIdentityProfile, MaterialPatchInput, MaterialPatchOutcome, UserRole,
    test_helpers::{insert_command_fixture, insert_printer_fixture},
};

mod jobs;
mod onboarding;
mod personal_presets;
mod printers;
mod recovery;
mod studio_contract;
mod studio_locking;
mod studio_queries;

pub(super) async fn postgres_database() -> Option<Database> {
    let url = match std::env::var("PANDAR_TEST_POSTGRES_URL") {
        Ok(url) => url,
        Err(_) => return None,
    };
    let config = DatabaseConfig::from_url(url).unwrap();
    let database = Database::connect(&config).await.unwrap();
    database.migrate().await.unwrap();
    clear_postgres(&database).await;
    Some(database)
}

pub(super) async fn clear_postgres(database: &Database) {
    let Database::Postgres(pool) = database else {
        panic!("expected PostgreSQL database");
    };
    sqlx::query(
        "TRUNCATE personal_presets, personal_preset_clocks, printer_event_tickets, audit_events, api_tokens, user_identities, join_links, tenant_tokens, plugin_login_tickets, job_filament_usages, printer_material_snapshots, machine_events, studio_submission_sequences, jobs, job_artifacts, commands, printers, agents, users, tenants",
    )
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn postgres_tenant_identity_listing_matches_sqlite_when_configured() {
    let Some(database) = postgres_database().await else {
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database);
    let tenant = tenants
        .create("identity-list", "Identity List")
        .await
        .unwrap();
    let other_tenant = tenants
        .create("other-identity-list", "Other Identity List")
        .await
        .unwrap();
    let user = auth
        .create_user(tenant.id, "viewer@example.test", "Viewer", UserRole::Viewer)
        .await
        .unwrap();
    let other_user = auth
        .create_user(
            other_tenant.id,
            "other@example.test",
            "Other",
            UserRole::Viewer,
        )
        .await
        .unwrap();
    let identity = auth
        .link_external_identity(tenant.id, &user.id, "clerk", "user_123")
        .await
        .unwrap();
    auth.link_external_identity(other_tenant.id, &other_user.id, "clerk", "other_user_123")
        .await
        .unwrap();

    assert_eq!(
        auth.list_external_identities_for_tenant(tenant.id)
            .await
            .unwrap(),
        vec![identity]
    );
}

#[tokio::test]
async fn postgres_material_patch_outcomes_match_sqlite_when_configured() {
    let Some(database) = postgres_database().await else {
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let materials = MaterialRepository::new(database.clone());
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();

    let changed = materials
        .upsert_from_patch_outcome(valid_material_input(
            tenant.id,
            agent.id,
            &printer_id,
            "2026-07-02T00:00:00Z",
        ))
        .await
        .unwrap();
    assert!(matches!(changed, MaterialPatchOutcome::Changed(_)));
    let unchanged = materials
        .upsert_from_patch_outcome(valid_material_input(
            tenant.id,
            agent.id,
            &printer_id,
            "2026-07-02T00:00:00Z",
        ))
        .await
        .unwrap();
    assert!(matches!(unchanged, MaterialPatchOutcome::Unchanged(_)));
    let older = materials
        .upsert_from_patch_outcome(valid_material_input(
            tenant.id,
            agent.id,
            &printer_id,
            "2026-07-01T00:00:00Z",
        ))
        .await
        .unwrap();
    assert!(matches!(older, MaterialPatchOutcome::Older));
}

fn valid_material_input(
    tenant_id: TenantId,
    agent_id: AgentId,
    printer_id: &str,
    observed_at: &str,
) -> MaterialPatchInput {
    MaterialPatchInput {
        tenant_id,
        agent_id,
        printer_id: printer_id.to_owned(),
        serial_number: format!("serial-{printer_id}"),
        printer_materials_json: serde_json::to_string(&PostgresMaterialPatchFixture {
            kind: "printer_material_patch",
            observed_at,
            ams_units: [PostgresMaterialPatchAmsUnit {
                unit_id: "0",
                trays: [PostgresMaterialPatchTray {
                    tray_id: "0",
                    material_type: "PLA",
                }],
            }],
            external_spools: [],
        })
        .unwrap(),
    }
}

#[derive(Debug, Serialize)]
struct PostgresMaterialPatchFixture<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    observed_at: &'a str,
    ams_units: [PostgresMaterialPatchAmsUnit; 1],
    external_spools: [(); 0],
}

#[derive(Debug, Serialize)]
struct PostgresMaterialPatchAmsUnit {
    unit_id: &'static str,
    trays: [PostgresMaterialPatchTray; 1],
}

#[derive(Debug, Serialize)]
struct PostgresMaterialPatchTray {
    tray_id: &'static str,
    #[serde(rename = "type")]
    material_type: &'static str,
}

#[tokio::test]
async fn postgres_core_repository_behavior_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let printers = PrinterRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());

    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let admin = auth
        .create_user(
            tenant.id,
            "postgres-admin@example.test",
            "Postgres Admin",
            UserRole::TenantAdmin,
        )
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    insert_command_fixture(&database, tenant.id, agent.id, Some(&printer_id))
        .await
        .unwrap();

    assert_eq!(tenants.list().await.unwrap(), vec![tenant.clone()]);
    assert_eq!(tenants.count().await.unwrap(), 1);
    assert_eq!(
        agents.list_for_tenant(tenant.id).await.unwrap(),
        vec![agent]
    );
    assert!(matches!(
        tenants.create("acme", "Acme Again").await.unwrap_err(),
        RepositoryError::DuplicateTenantSlug
    ));
    assert_eq!(printers.count().await.unwrap(), 1);
    assert_eq!(commands.count().await.unwrap(), 1);

    let stale = agents.create(tenant.id, "stale-agent").await.unwrap();
    let deleted = agents
        .delete_offline_with_audit(tenant.id, stale.id, AuditActor::user(admin.id.clone()))
        .await
        .unwrap();
    assert_eq!(deleted, stale);
    assert_eq!(agents.get(stale.id).await.unwrap(), None);
}

#[tokio::test]
async fn postgres_cleanup_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    crate::repositories::tests::cleanup::exercise_cleanup(
        database.clone(),
        TenantRepository::new(database.clone()),
        AgentRepository::new(database.clone()),
        CommandRepository::new(database.clone()),
        JobRepository::new(database),
    )
    .await;
}

#[tokio::test]
async fn postgres_partial_snapshot_preserves_absent_telemetry_fields_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    super::printer_snapshot_presence::exercise_partial_snapshot_presence(database).await;
}

#[tokio::test]
async fn postgres_mqtt_presence_requires_an_authoritative_current_session_snapshot_when_configured()
{
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    super::printer_snapshot_presence::exercise_mqtt_presence_session(database).await;
}

#[tokio::test]
async fn postgres_print_reports_merge_printer_live_status_without_a_job_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    super::printer_live_status::exercise_web_print_monitor_schema(database.clone()).await;
    super::printer_live_status::exercise_printer_live_status(database.clone()).await;
    super::printer_live_status::revisions::exercise_atomic_revisions(database.clone()).await;
    super::printer_live_status::revisions::exercise_concurrent_revision_writers(
        database,
        "postgres-revision-race",
    )
    .await;
}

#[tokio::test]
async fn postgres_printer_access_code_migration_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database.clone());
    let tenant = tenants
        .create("legacy-secret", "Legacy Secret")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    let Database::Postgres(pool) = &database else {
        panic!("expected PostgreSQL database");
    };
    sqlx::query("UPDATE printers SET access_code = 'legacy-code' WHERE id = $1")
        .bind(&printer_id)
        .execute(pool)
        .await
        .unwrap();

    crate::printer_secrets::migrate_printer_access_codes(&database, &printers.access_code_cipher())
        .await
        .unwrap();

    let (plaintext, encrypted): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT access_code, access_code_encrypted FROM printers WHERE id = $1")
            .bind(&printer_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(plaintext, None);
    assert!(
        encrypted
            .as_deref()
            .is_some_and(|value| value.starts_with("v1:"))
    );
    assert_eq!(
        printers
            .get_for_tenant(tenant.id, &printer_id)
            .await
            .unwrap()
            .unwrap()
            .access_code
            .as_deref(),
        Some("legacy-code")
    );
}

#[tokio::test]
async fn printer_device_features_postgres_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    super::printer_device_features::exercise_printer_device_features(database).await;
}

#[tokio::test]
async fn printer_device_features_postgres_migrates_legacy_rows_when_configured() {
    let Some((database, admin, schema)) = isolated_postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    super::printer_device_features::exercise_legacy_device_features_migration(database.clone())
        .await;
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

async fn isolated_postgres_database() -> Option<(Database, sqlx::PgPool, String)> {
    let url = match std::env::var("PANDAR_TEST_POSTGRES_URL") {
        Ok(url) => url,
        Err(_) => return None,
    };
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let schema = format!("pandar_device_features_{}", uuid::Uuid::new_v4().simple());
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

    Some((Database::Postgres(pool), admin, schema))
}
