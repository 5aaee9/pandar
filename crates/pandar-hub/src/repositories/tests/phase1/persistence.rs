use super::*;

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
