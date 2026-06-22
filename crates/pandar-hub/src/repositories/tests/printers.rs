use pandar_core::{AgentId, TenantId};

use super::*;
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
        name: name.to_string(),
        model: model.map(str::to_string),
        status: status.to_string(),
        observed_at: observed_at.to_string(),
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
    let updated = printers
        .upsert_snapshot(
            acme.id,
            acme_agent.id,
            snapshot(
                "SN-001",
                "Renamed Printer",
                Some("X1 Carbon"),
                "printing",
                "2026-06-21T01:00:00Z",
            ),
        )
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
    assert_eq!(updated.name, "Renamed Printer");
    assert_eq!(updated.model.as_deref(), Some("X1 Carbon"));
    assert_eq!(updated.status, "printing");
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
