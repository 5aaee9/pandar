use super::*;

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
