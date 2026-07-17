use std::str::FromStr;

use serde::Serialize;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use super::*;
use crate::repositories::{
    AuditActor, ExternalIdentityProfile, MaterialPatchInput, MaterialPatchOutcome, UserRole,
    test_helpers::{insert_command_fixture, insert_printer_fixture},
};

mod jobs;
mod recovery;

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
        "TRUNCATE printer_event_tickets, audit_events, api_tokens, user_identities, join_links, tenant_tokens, plugin_login_tickets, job_filament_usages, printer_material_snapshots, machine_events, jobs, job_artifacts, commands, printers, agents, users, tenants",
    )
        .execute(pool)
        .await
        .unwrap();
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
async fn postgres_external_onboarding_behavior_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let auth = AuthRepository::new(database.clone());
    let audit = AuditEventRepository::new(database);
    let admin = auth
        .self_create_tenant_for_external_identity(
            "pg-onboarding",
            "Postgres Onboarding",
            ExternalIdentityProfile {
                provider: "betterauth".to_owned(),
                subject: "admin-subject".to_owned(),
                email: "admin@example.test".to_owned(),
                display_name: "Admin".to_owned(),
            },
        )
        .await
        .unwrap();
    assert_eq!(admin.user.role, UserRole::TenantAdmin);

    let memberships = auth
        .list_external_memberships("betterauth", "admin-subject")
        .await
        .unwrap();
    assert_eq!(memberships.len(), 1);
    assert_eq!(memberships[0].tenant.id, admin.tenant.id);

    let link = auth
        .create_join_link_with_audit(
            admin.tenant.id,
            UserRole::Operator,
            Some("operator@example.test".to_owned()),
            60,
            1,
            AuditActor::user(admin.user.id.clone()),
        )
        .await
        .unwrap();
    let accepted = auth
        .accept_join_link(
            &link.plaintext_token,
            ExternalIdentityProfile {
                provider: "betterauth".to_owned(),
                subject: "operator-subject".to_owned(),
                email: "operator@example.test".to_owned(),
                display_name: "Operator".to_owned(),
            },
        )
        .await
        .unwrap();
    assert!(accepted.created);
    assert_eq!(accepted.user.role, UserRole::Operator);

    let existing_link = auth
        .create_join_link_with_audit(
            admin.tenant.id,
            UserRole::Viewer,
            Some("changed@example.test".to_owned()),
            60,
            1,
            AuditActor::user(admin.user.id.clone()),
        )
        .await
        .unwrap();
    let existing = auth
        .accept_join_link(
            &existing_link.plaintext_token,
            ExternalIdentityProfile {
                provider: "betterauth".to_owned(),
                subject: "operator-subject".to_owned(),
                email: "changed@example.test".to_owned(),
                display_name: "Operator Changed".to_owned(),
            },
        )
        .await
        .unwrap();
    assert!(!existing.created);
    assert_eq!(existing.user.id, accepted.user.id);
    assert_eq!(existing.user.role, UserRole::Operator);

    let listed = auth
        .list_join_links_for_tenant(admin.tenant.id)
        .await
        .unwrap();
    assert!(
        listed
            .iter()
            .any(|join_link| join_link.id == link.join_link.id && join_link.used_count == 1)
    );
    assert!(
        listed.iter().any(
            |join_link| join_link.id == existing_link.join_link.id && join_link.used_count == 0
        )
    );
    let revoked = auth
        .create_join_link_with_audit(
            admin.tenant.id,
            UserRole::Viewer,
            None,
            60,
            1,
            AuditActor::user(admin.user.id.clone()),
        )
        .await
        .unwrap();
    let revoked = auth
        .revoke_join_link_with_audit(
            admin.tenant.id,
            &revoked.join_link.id,
            AuditActor::user(admin.user.id.clone()),
        )
        .await
        .unwrap();
    assert!(revoked.revoked_at.is_some());

    let concurrent = auth
        .create_join_link_with_audit(
            admin.tenant.id,
            UserRole::Viewer,
            None,
            60,
            1,
            AuditActor::user(admin.user.id.clone()),
        )
        .await
        .unwrap();
    super::auth::assert_single_concurrent_accept(
        auth.clone(),
        admin.tenant.id,
        concurrent.join_link.id,
        concurrent.plaintext_token,
    )
    .await;

    let events = audit.list_for_tenant(admin.tenant.id).await.unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.action == "join_link.accept")
    );
    let audit_json = events
        .iter()
        .map(|event| event.metadata_json.as_str())
        .collect::<String>();
    assert!(!audit_json.contains("admin-subject"));
    assert!(!audit_json.contains("operator-subject"));
    assert!(!audit_json.contains(&link.plaintext_token));
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
async fn postgres_printer_repository_upsert_list_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database.clone());
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();

    let created = printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            PrinterSnapshotUpsert {
                serial_number: "SN-001".to_string(),
                host: Some("192.0.2.10".to_string()),
                access_code: Some("12345678".to_string()),
                name: "Garage A1".to_string(),
                model: Some("A1 Mini".to_string()),
                status: "idle".to_string(),
                observed_at: "2026-06-21T00:00:00Z".to_string(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_light_on: None,
                connection_authoritative: false,
            },
        )
        .await
        .unwrap();
    printers
        .update_details_with_audit(
            tenant.id,
            &created.id,
            "Garage A1".to_owned(),
            "192.0.2.11".to_owned(),
            "edited-access-code".to_owned(),
            AuditActor::no_auth(),
        )
        .await
        .unwrap();
    let updated = printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            PrinterSnapshotUpsert {
                serial_number: "SN-001".to_string(),
                host: Some("192.0.2.10".to_owned()),
                access_code: Some("12345678".to_owned()),
                name: "Ignored Snapshot Name".to_string(),
                model: Some("A1 Mini".to_string()),
                status: "printing".to_string(),
                observed_at: "2026-06-21T00:05:00Z".to_string(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_light_on: None,
                connection_authoritative: false,
            },
        )
        .await
        .unwrap();

    assert_eq!(updated.id, created.id);
    assert_eq!(updated.created_at, created.created_at);
    assert_eq!(updated.name, "Garage A1");
    assert_eq!(updated.host.as_deref(), Some("192.0.2.11"));
    assert_eq!(updated.access_code.as_deref(), Some("edited-access-code"));
    assert_eq!(updated.status, "printing");
    assert_eq!(updated.last_seen_at, "2026-06-21T00:05:00Z");
    let Database::Postgres(pool) = &database else {
        panic!("expected PostgreSQL database");
    };
    let (plaintext, encrypted): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT access_code, access_code_encrypted FROM printers WHERE id = $1")
            .bind(&created.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(plaintext, None);
    assert!(
        encrypted
            .as_deref()
            .is_some_and(|value| value.starts_with("v1:"))
    );

    let authoritative = printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            PrinterSnapshotUpsert {
                serial_number: "SN-001".to_string(),
                host: Some("192.0.2.12".to_owned()),
                access_code: Some("reloaded-access-code".to_owned()),
                name: "Ignored Snapshot Name".to_string(),
                model: Some("A1 Mini".to_string()),
                status: "idle".to_string(),
                observed_at: "2026-06-21T00:10:00Z".to_string(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_light_on: None,
                connection_authoritative: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(authoritative.host.as_deref(), Some("192.0.2.12"));
    assert_eq!(
        authoritative.access_code.as_deref(),
        Some("reloaded-access-code")
    );
    assert_eq!(
        printers.list_for_tenant(tenant.id).await.unwrap(),
        vec![authoritative]
    );
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
