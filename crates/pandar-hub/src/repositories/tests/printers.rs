use pandar_core::{AgentId, TenantId};

use super::*;
use crate::db::DatabaseConfig;
use crate::repositories::AuditActor;
use crate::repositories::test_helpers::insert_printer_fixture;

fn snapshot(
    serial_number: &str,
    name: &str,
    model: Option<&str>,
    status: &str,
    observed_at: &str,
) -> PrinterSnapshotUpsert {
    PrinterSnapshotUpsert {
        serial_number: serial_number.to_string(),
        host: Some("192.0.2.10".to_string()),
        access_code: Some("test-access-code".to_string()),
        name: name.to_string(),
        model: model.map(str::to_string),
        status: status.to_string(),
        observed_at: observed_at.to_string(),
        nozzle_temperatures: Vec::new(),
        active_nozzle: None,
        bed_temperature_celsius: None,
        bed_target_temperature_celsius: None,
        chamber_temperature_celsius: None,
        chamber_target_temperature_celsius: None,
        chamber_light_on: None,
        connection_authoritative: false,
    }
}

fn telemetry_snapshot_without_connection(
    serial_number: &str,
    name: &str,
    model: Option<&str>,
    status: &str,
    observed_at: &str,
) -> PrinterSnapshotUpsert {
    PrinterSnapshotUpsert {
        host: None,
        access_code: None,
        ..snapshot(serial_number, name, model, status, observed_at)
    }
}

#[tokio::test]
async fn printer_repository_upserts_and_lists_for_tenant() {
    let (_, tenants, agents, printers, _, _) = repositories().await;
    let acme = tenants.create("acme", "Acme Labs").await.unwrap();
    let beta = tenants.create("beta", "Beta Labs").await.unwrap();
    let acme_agent = agents.create(acme.id, "agent").await.unwrap();
    let beta_agent = agents.create(beta.id, "agent").await.unwrap();

    let created = printers
        .upsert_snapshot(
            acme.id,
            acme_agent.id,
            snapshot(
                "SN-001",
                "First Printer",
                Some("X1C"),
                "offline",
                "2026-06-21T00:00:00Z",
            ),
        )
        .await
        .unwrap();
    let mut updated_snapshot = snapshot(
        "SN-001",
        "Renamed Printer",
        Some("X1 Carbon"),
        "printing",
        "2026-06-21T01:00:00Z",
    );
    updated_snapshot.chamber_target_temperature_celsius = Some("45".to_owned());
    let updated = printers
        .upsert_snapshot(acme.id, acme_agent.id, updated_snapshot)
        .await
        .unwrap();
    printers
        .upsert_snapshot(
            beta.id,
            beta_agent.id,
            snapshot(
                "SN-001",
                "Beta Printer",
                None,
                "offline",
                "2026-06-21T02:00:00Z",
            ),
        )
        .await
        .unwrap();

    assert_eq!(updated.id, created.id);
    assert_eq!(updated.created_at, created.created_at);
    assert_eq!(updated.name, "First Printer");
    assert_eq!(updated.model.as_deref(), Some("X1 Carbon"));
    assert_eq!(updated.status, "printing");
    assert_eq!(
        updated.chamber_target_temperature_celsius.as_deref(),
        Some("45")
    );
    assert_eq!(updated.last_seen_at, "2026-06-21T01:00:00Z");
    assert_eq!(printers.count().await.unwrap(), 2);
    assert_eq!(
        printers.list_for_tenant(acme.id).await.unwrap(),
        vec![updated]
    );
}

#[tokio::test]
async fn printer_repository_get_returns_none_for_unknown_printer() {
    let (_, tenants, _, printers, _, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();

    assert_eq!(
        printers
            .get_for_tenant(tenant.id, "missing-printer")
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn printer_repository_updates_connection_details() {
    let (database, tenants, agents, printers, _, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();

    let updated = printers
        .update_details_with_audit(
            tenant.id,
            &printer_id,
            "Office A1".to_string(),
            "192.0.2.12".to_string(),
            "updated-access-code".to_string(),
            AuditActor::no_auth(),
        )
        .await
        .unwrap();

    assert_eq!(updated.name, "Office A1");
    assert_eq!(updated.host.as_deref(), Some("192.0.2.12"));
    assert_eq!(updated.access_code.as_deref(), Some("updated-access-code"));
    assert_eq!(
        printers
            .get_for_tenant(tenant.id, &printer_id)
            .await
            .unwrap()
            .unwrap(),
        updated
    );

    let Database::Sqlite(pool) = &database else {
        panic!("expected SQLite database");
    };
    let (plaintext, encrypted): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT access_code, access_code_encrypted FROM printers WHERE id = ?1")
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
    assert!(!encrypted.unwrap().contains("updated-access-code"));
}

#[tokio::test]
async fn printer_access_code_migration_encrypts_legacy_plaintext() {
    let (database, tenants, agents, printers, _, _) = repositories().await;
    let tenant = tenants.create("legacy", "Legacy").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    let Database::Sqlite(pool) = &database else {
        panic!("expected SQLite database");
    };
    sqlx::query("UPDATE printers SET access_code = 'legacy-code' WHERE id = ?1")
        .bind(&printer_id)
        .execute(pool)
        .await
        .unwrap();

    crate::printer_secrets::migrate_printer_access_codes(&database, &printers.access_code_cipher())
        .await
        .unwrap();

    let (plaintext, encrypted): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT access_code, access_code_encrypted FROM printers WHERE id = ?1")
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
async fn printer_repository_rejects_stale_connection_snapshot_after_edit() {
    let (_, tenants, agents, printers, _, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let created = printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            snapshot(
                "SN-001",
                "Printer",
                Some("X1C"),
                "idle",
                "2026-06-21T00:00:00Z",
            ),
        )
        .await
        .unwrap();
    printers
        .update_details_with_audit(
            tenant.id,
            &created.id,
            "Printer".to_owned(),
            "192.0.2.11".to_owned(),
            "edited-access-code".to_owned(),
            AuditActor::no_auth(),
        )
        .await
        .unwrap();

    let stale = printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            snapshot(
                "SN-001",
                "Printer",
                Some("X1C"),
                "printing",
                "2026-06-21T00:05:00Z",
            ),
        )
        .await
        .unwrap();

    assert_eq!(stale.host.as_deref(), Some("192.0.2.11"));
    assert_eq!(stale.access_code.as_deref(), Some("edited-access-code"));
    assert_eq!(stale.status, "printing");
}

#[tokio::test]
async fn printer_repository_accepts_authoritative_connection_snapshot() {
    let (_, tenants, agents, printers, _, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            snapshot(
                "SN-001",
                "Printer",
                Some("X1C"),
                "idle",
                "2026-06-21T00:00:00Z",
            ),
        )
        .await
        .unwrap();
    let mut authoritative = snapshot(
        "SN-001",
        "Printer",
        Some("X1C"),
        "idle",
        "2026-06-21T00:05:00Z",
    );
    authoritative.host = Some("192.0.2.12".to_owned());
    authoritative.access_code = Some("reloaded-access-code".to_owned());
    authoritative.connection_authoritative = true;

    let reloaded = printers
        .upsert_snapshot(tenant.id, agent.id, authoritative)
        .await
        .unwrap();

    assert_eq!(reloaded.host.as_deref(), Some("192.0.2.12"));
    assert_eq!(
        reloaded.access_code.as_deref(),
        Some("reloaded-access-code")
    );
}

#[tokio::test]
async fn printer_repository_snapshot_without_connection_preserves_saved_connection() {
    let (_, tenants, agents, printers, _, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();

    printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            snapshot(
                "SN-001",
                "Printer",
                Some("X1C"),
                "idle",
                "2026-06-21T00:00:00Z",
            ),
        )
        .await
        .unwrap();
    let updated = printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            telemetry_snapshot_without_connection(
                "SN-001",
                "Printer",
                Some("X1 Carbon"),
                "printing",
                "2026-06-21T00:05:00Z",
            ),
        )
        .await
        .unwrap();

    assert_eq!(updated.host.as_deref(), Some("192.0.2.10"));
    assert_eq!(updated.access_code.as_deref(), Some("test-access-code"));
    assert_eq!(updated.model.as_deref(), Some("X1 Carbon"));
    assert_eq!(updated.status, "printing");
}

#[tokio::test]
async fn printer_repository_list_rejects_missing_tenant() {
    let (_, _, _, printers, _, _) = repositories().await;

    let err = printers.list_for_tenant(TenantId::new()).await.unwrap_err();

    assert!(matches!(err, RepositoryError::MissingTenant));
}

#[tokio::test]
async fn printer_repository_reassigns_serial_to_latest_agent() {
    let (_, tenants, agents, printers, _, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let first_agent = agents.create(tenant.id, "first").await.unwrap();
    let second_agent = agents.create(tenant.id, "second").await.unwrap();

    let created = printers
        .upsert_snapshot(
            tenant.id,
            first_agent.id,
            snapshot("SN-001", "Printer", None, "idle", "2026-06-21T00:00:00Z"),
        )
        .await
        .unwrap();
    let reassigned = printers
        .upsert_snapshot(
            tenant.id,
            second_agent.id,
            snapshot("SN-001", "Printer", None, "idle", "2026-06-21T00:05:00Z"),
        )
        .await
        .unwrap();

    assert_eq!(reassigned.id, created.id);
    assert_eq!(reassigned.agent_id, second_agent.id);
}

#[tokio::test]
async fn printer_repository_concurrent_duplicate_serial_upserts_are_atomic() {
    let temp_dir = tempfile::tempdir().unwrap();
    let database_path = temp_dir.path().join("concurrent-printers.sqlite");
    let database_url = format!("sqlite://{}", database_path.display());
    let config = DatabaseConfig::from_url(database_url).unwrap();
    let database = Database::connect(&config).await.unwrap();
    database.migrate().await.unwrap();

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database.clone());
    let tenant = tenants
        .create("acme-concurrent", "Acme Labs")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();

    let first_printers = printers.clone();
    let second_printers = PrinterRepository::new(database.clone());
    let first = tokio::spawn(async move {
        first_printers
            .upsert_snapshot(
                tenant.id,
                agent.id,
                snapshot(
                    "SN-CONCURRENT",
                    "First Concurrent",
                    Some("X1C"),
                    "idle",
                    "2026-06-21T00:00:00Z",
                ),
            )
            .await
    });
    let second = tokio::spawn(async move {
        second_printers
            .upsert_snapshot(
                tenant.id,
                agent.id,
                snapshot(
                    "SN-CONCURRENT",
                    "Second Concurrent",
                    Some("X1 Carbon"),
                    "printing",
                    "2026-06-21T00:00:01Z",
                ),
            )
            .await
    });

    let first = first.await.unwrap().unwrap();
    let second = second.await.unwrap().unwrap();

    assert_eq!(first.id, second.id);
    assert_eq!(printers.count().await.unwrap(), 1);
    let listed = printers.list_for_tenant(tenant.id).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(["First Concurrent", "Second Concurrent"].contains(&listed[0].name.as_str()));
}

#[tokio::test]
async fn printer_repository_rejects_missing_agent() {
    let (_, tenants, agents, printers, _, _) = repositories().await;
    let acme = tenants.create("acme", "Acme Labs").await.unwrap();
    let beta = tenants.create("beta", "Beta Labs").await.unwrap();
    let beta_agent = agents.create(beta.id, "agent").await.unwrap();

    let missing_err = printers
        .upsert_snapshot(
            acme.id,
            AgentId::new(),
            snapshot("SN-001", "Printer", None, "idle", "2026-06-21T00:00:00Z"),
        )
        .await
        .unwrap_err();
    let wrong_tenant_err = printers
        .upsert_snapshot(
            acme.id,
            beta_agent.id,
            snapshot("SN-002", "Printer", None, "idle", "2026-06-21T00:00:00Z"),
        )
        .await
        .unwrap_err();

    assert!(matches!(missing_err, RepositoryError::MissingAgent));
    assert!(matches!(wrong_tenant_err, RepositoryError::MissingAgent));
    assert_eq!(printers.count().await.unwrap(), 0);
}

#[tokio::test]
async fn invalid_persisted_printer_status_is_reported_with_context() {
    let (database, tenants, agents, printers, _, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();

    let Database::Sqlite(pool) = &database else {
        panic!("expected SQLite database");
    };
    sqlx::query("UPDATE printers SET status = '' WHERE id = ?1")
        .bind(&printer_id)
        .execute(pool)
        .await
        .unwrap();

    let err = printers.list_for_tenant(tenant.id).await.unwrap_err();

    assert!(matches!(err, RepositoryError::Database(_)));
    assert!(format!("{err:#}").contains("failed to rehydrate printer"));
}

#[tokio::test]
async fn invalid_persisted_printer_device_features_are_reported_with_context() {
    let (database, tenants, agents, printers, _, _) = repositories().await;
    let tenant = tenants
        .create("feature-context", "Feature Context")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();

    let Database::Sqlite(pool) = &database else {
        panic!("expected SQLite database");
    };
    sqlx::query("UPDATE printers SET bambu_fun_bits = 'not-hex' WHERE id = ?1")
        .bind(&printer_id)
        .execute(pool)
        .await
        .unwrap();

    let err = printers
        .get_for_tenant(tenant.id, &printer_id)
        .await
        .unwrap_err();
    let chain = format!("{err:#}");
    assert!(chain.contains("failed to rehydrate printer Bambu device features"));
    assert!(chain.contains("device feature bitmap contains non-hexadecimal characters"));
}
