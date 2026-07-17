use pandar_core::{AgentId, AgentStatus, TenantId};
use serde::Deserialize;

use super::*;
use crate::{
    db::{Database, DatabaseBackend, DatabaseConfig},
    repositories::test_helpers::{insert_command_fixture, insert_printer_fixture},
};

#[tokio::test]
async fn sqlite_migrations_create_phase_1_schema() {
    let database = sqlite_database().await;
    let Database::Sqlite(pool) = database else {
        panic!("expected SQLite database");
    };

    for table in [
        "tenants",
        "users",
        "agents",
        "printers",
        "commands",
        "job_artifacts",
        "jobs",
    ] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "{table} table should exist");
    }

    let index_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_users_tenant_id'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(index_count, 1);

    let command_index_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_commands_agent_status'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(command_index_count, 1);

    let printer_last_seen_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('printers') WHERE name = ?1")
            .bind("last_seen_at")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(printer_last_seen_count, 1);

    let jobs_command_id_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('jobs') WHERE name = ?1")
            .bind("command_id")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(jobs_command_id_count, 1);

    for column in ["studio_cfg", "studio_aux", "studio_stat"] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('printer_material_snapshots') WHERE name = ?1",
        )
        .bind(column)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "{column} column should exist");
    }
}

#[test]
fn phase_13_command_result_migrations_are_backend_equivalent() {
    let sqlite =
        include_str!("../../../migrations/sqlite/20260623000000_phase_13_command_results.sql");
    let postgres =
        include_str!("../../../migrations/postgres/20260623000000_phase_13_command_results.sql");

    assert_eq!(
        sqlite.trim(),
        "ALTER TABLE commands ADD COLUMN result_json TEXT;"
    );
    assert_eq!(postgres.trim(), sqlite.trim());
}

#[test]
fn phase_14_material_migrations_are_backend_equivalent() {
    let sqlite = include_str!("../../../migrations/sqlite/20260623010000_phase_14_materials.sql");
    let postgres =
        include_str!("../../../migrations/postgres/20260623010000_phase_14_materials.sql");

    assert_eq!(postgres.trim(), sqlite.trim());
    for required in [
        "ALTER TABLE jobs ADD COLUMN ams_mapping_json TEXT;",
        "ALTER TABLE jobs ADD COLUMN ams_mapping2_json TEXT;",
        "CREATE TABLE printer_material_snapshots",
        "ams_json TEXT NOT NULL",
        "external_spools_json TEXT NOT NULL",
        "active_tray_json TEXT",
        "UNIQUE (tenant_id, printer_id)",
        "idx_printer_material_snapshots_tenant_printer",
        "idx_printer_material_snapshots_tenant_serial",
        "CREATE TABLE job_filament_usages",
        "source TEXT NOT NULL CHECK (source IN ('ams_mapping2', 'ams_mapping'))",
        "confidence TEXT NOT NULL CHECK (confidence IN ('mapped_no_quantity', 'report_estimate'))",
        "UNIQUE (tenant_id, job_id, slot_index, source)",
        "idx_job_filament_usages_tenant_job",
        "idx_job_filament_usages_tenant_job_slot",
    ] {
        assert!(
            sqlite.contains(required),
            "missing migration fragment: {required}"
        );
    }
}

#[test]
fn refresh_printer_materials_requires_no_new_migration_files() {
    let sqlite = include_str!("../../../migrations/sqlite/20260623010000_phase_14_materials.sql");
    let postgres =
        include_str!("../../../migrations/postgres/20260623010000_phase_14_materials.sql");

    assert!(sqlite.contains("printer_material_snapshots"));
    assert!(postgres.contains("printer_material_snapshots"));
}

#[test]
fn filament_switch_state_migrations_are_backend_equivalent() {
    let sqlite =
        include_str!("../../../migrations/sqlite/20260716000000_filament_switch_state.sql");
    let postgres =
        include_str!("../../../migrations/postgres/20260716000000_filament_switch_state.sql");

    for migration in [sqlite, postgres] {
        assert!(migration.contains("ALTER TABLE printer_material_snapshots"));
        assert!(migration.contains("ADD COLUMN filament_switch_installed"));
    }
    assert!(sqlite.contains("INTEGER"));
    assert!(postgres.contains("BOOLEAN"));
}

#[test]
fn studio_status_flag_migrations_are_backend_equivalent() {
    let sqlite = include_str!("../../../migrations/sqlite/20260716010000_studio_status_flags.sql");
    let postgres =
        include_str!("../../../migrations/postgres/20260716010000_studio_status_flags.sql");

    assert_eq!(postgres.trim(), sqlite.trim());
    for column in ["studio_cfg", "studio_aux", "studio_stat"] {
        assert!(sqlite.contains(&format!(
            "ALTER TABLE printer_material_snapshots ADD COLUMN {column} TEXT;"
        )));
    }
}

#[test]
fn printer_access_code_encryption_migrations_are_backend_equivalent() {
    let sqlite =
        include_str!("../../../migrations/sqlite/20260718000000_encrypt_printer_access_codes.sql");
    let postgres = include_str!(
        "../../../migrations/postgres/20260718000000_encrypt_printer_access_codes.sql"
    );

    assert_eq!(postgres.trim(), sqlite.trim());
    assert_eq!(
        sqlite.trim(),
        "ALTER TABLE printers ADD COLUMN access_code_encrypted TEXT;"
    );
}

#[test]
fn phase_28_slicer_metadata_migrations_are_backend_equivalent() {
    let sqlite =
        include_str!("../../../migrations/sqlite/20260624010000_phase_28_slicer_metadata.sql");
    let postgres =
        include_str!("../../../migrations/postgres/20260624010000_phase_28_slicer_metadata.sql");

    assert_eq!(postgres.trim(), sqlite.trim());
    assert_eq!(
        sqlite.trim(),
        "ALTER TABLE job_artifacts ADD COLUMN metadata_json TEXT;"
    );
}

#[tokio::test]
async fn tenant_create_list_and_count_work() {
    let (_, tenants, _, _, _, _) = repositories().await;

    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    assert_eq!(tenant.slug, "acme");
    assert_eq!(tenants.count().await.unwrap(), 1);
    assert_eq!(tenants.list().await.unwrap(), vec![tenant]);
}

#[tokio::test]
async fn duplicate_tenant_slug_is_rejected() {
    let (_, tenants, _, _, _, _) = repositories().await;

    tenants.create("acme", "Acme Labs").await.unwrap();
    let err = tenants.create("acme", "Acme Again").await.unwrap_err();

    assert!(matches!(err, RepositoryError::DuplicateTenantSlug));
}

#[tokio::test]
async fn agent_create_and_list_are_scoped_to_tenant() {
    let (_, tenants, agents, _, _, _) = repositories().await;
    let acme = tenants.create("acme", "Acme Labs").await.unwrap();
    let beta = tenants.create("beta", "Beta Labs").await.unwrap();

    let acme_agent = agents.create(acme.id, "shop-floor").await.unwrap();
    agents.create(beta.id, "shop-floor").await.unwrap();

    assert_eq!(agents.count().await.unwrap(), 2);
    assert_eq!(
        agents.list_for_tenant(acme.id).await.unwrap(),
        vec![acme_agent]
    );
}

#[tokio::test]
async fn duplicate_agent_name_is_rejected_within_tenant() {
    let (_, tenants, agents, _, _, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();

    agents.create(tenant.id, "shop-floor").await.unwrap();
    let err = agents.create(tenant.id, "shop-floor").await.unwrap_err();

    assert!(matches!(err, RepositoryError::DuplicateAgentName));
}

#[tokio::test]
async fn missing_tenant_is_reported_for_agent_create_and_list() {
    let (_, _, agents, _, _, _) = repositories().await;
    let missing = TenantId::new();

    let create_err = agents.create(missing, "agent").await.unwrap_err();
    assert!(matches!(create_err, RepositoryError::MissingTenant));

    let list_err = agents.list_for_tenant(missing).await.unwrap_err();
    assert!(matches!(list_err, RepositoryError::MissingTenant));
}

#[tokio::test]
async fn invalid_persisted_agent_status_is_reported() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();

    let Database::Sqlite(pool) = &database else {
        panic!("expected SQLite database");
    };
    sqlx::query("UPDATE agents SET status = 'strange' WHERE id = ?1")
        .bind(agent.id.to_string())
        .execute(pool)
        .await
        .unwrap();

    let err = agents.list_for_tenant(tenant.id).await.unwrap_err();
    assert!(matches!(err, RepositoryError::InvalidPersistedStatus(status) if status == "strange"));
}

#[tokio::test]
async fn agent_get_update_connection_and_mark_offline_work() {
    let (_, tenants, agents, _, _, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();

    assert_eq!(agents.get(agent.id).await.unwrap(), Some(agent.clone()));
    let online = agents
        .update_connection(
            agent.id,
            AgentStatus::Online,
            Some("0.2.0"),
            "2026-06-20T01:00:00Z",
        )
        .await
        .unwrap();
    assert_eq!(online.status, AgentStatus::Online);

    let offline = agents
        .mark_offline(agent.id, "2026-06-20T01:01:00Z")
        .await
        .unwrap();
    assert_eq!(offline.status, AgentStatus::Offline);
    assert_eq!(agents.get(AgentId::new()).await.unwrap(), None);
}

#[tokio::test]
async fn agent_delete_offline_removes_agent_cascades_and_audits() {
    let (database, tenants, agents, printers, commands, _) = repositories().await;
    let tenant = tenants.create("delete-acme", "Delete Acme").await.unwrap();
    let admin = AuthRepository::new(database.clone())
        .create_user(
            tenant.id,
            "delete-admin@example.test",
            "Delete Admin",
            crate::repositories::UserRole::TenantAdmin,
        )
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "stale-agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    insert_command_fixture(&database, tenant.id, agent.id, Some(&printer_id))
        .await
        .unwrap();

    let deleted = agents
        .delete_offline_with_audit(
            tenant.id,
            agent.id,
            crate::repositories::AuditActor::user(admin.id.clone()),
        )
        .await
        .unwrap();

    assert_eq!(deleted, agent);
    assert_eq!(agents.get(agent.id).await.unwrap(), None);
    assert_eq!(printers.count().await.unwrap(), 0);
    assert_eq!(commands.count().await.unwrap(), 0);

    let audit = AuditEventRepository::new(database);
    let events = audit.list_for_tenant(tenant.id).await.unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "agent.delete")
        .expect("agent delete audit event");
    assert_eq!(event.target_type, "agent");
    assert_eq!(
        event.target_id.as_deref(),
        Some(agent.id.to_string().as_str())
    );
    let metadata: TestAgentDeleteAuditMetadata =
        serde_json::from_str(&event.metadata_json).unwrap();
    assert_eq!(
        metadata,
        TestAgentDeleteAuditMetadata {
            agent_name: "stale-agent".to_owned(),
            previous_status: "offline".to_owned(),
        }
    );
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TestAgentDeleteAuditMetadata {
    agent_name: String,
    previous_status: String,
}

#[tokio::test]
async fn agent_delete_rejects_online_agent() {
    let (_, tenants, agents, _, _, _) = repositories().await;
    let tenant = tenants.create("online-acme", "Online Acme").await.unwrap();
    let agent = agents.create(tenant.id, "online-agent").await.unwrap();
    agents
        .update_connection(
            agent.id,
            AgentStatus::Online,
            Some("0.2.0"),
            "2026-06-20T01:00:00Z",
        )
        .await
        .unwrap();

    let err = agents
        .delete_offline_with_audit(
            tenant.id,
            agent.id,
            crate::repositories::AuditActor::user("test-user"),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::AgentOnline));
    assert_eq!(
        agents.get(agent.id).await.unwrap().unwrap().status,
        AgentStatus::Online
    );
}

#[tokio::test]
async fn summary_counts_include_printer_and_command_fixtures() {
    let (database, tenants, agents, printers, commands, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    insert_command_fixture(&database, tenant.id, agent.id, Some(&printer_id))
        .await
        .unwrap();

    assert_eq!(tenants.count().await.unwrap(), 1);
    assert_eq!(agents.count().await.unwrap(), 1);
    assert_eq!(printers.count().await.unwrap(), 1);
    assert_eq!(commands.count().await.unwrap(), 1);
}

#[tokio::test]
async fn file_sqlite_records_survive_reconnect() {
    let temp_dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", temp_dir.path().join("pandar.db").display());

    let config = DatabaseConfig::from_url(&url).unwrap();
    let database = Database::connect(&config).await.unwrap();
    database.migrate().await.unwrap();
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    agents.create(tenant.id, "agent").await.unwrap();
    drop(database);

    let database = Database::connect(&config).await.unwrap();
    database.migrate().await.unwrap();
    assert_eq!(
        TenantRepository::new(database.clone())
            .count()
            .await
            .unwrap(),
        1
    );
    assert_eq!(AgentRepository::new(database).count().await.unwrap(), 1);
}

#[tokio::test]
async fn sqlite_memory_keeps_migrations_and_queries_on_same_database() {
    let (database, tenants, _, _, _, _) = repositories().await;

    assert_eq!(database.backend(), DatabaseBackend::Sqlite);
    tenants.create("acme", "Acme Labs").await.unwrap();
    assert_eq!(tenants.count().await.unwrap(), 1);
}
