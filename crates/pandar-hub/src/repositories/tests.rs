use pandar_core::TenantId;

use super::{
    AgentRepository, CommandRepository, PrinterRepository, RepositoryError, TenantRepository,
    test_helpers::{insert_command_fixture, insert_printer_fixture},
};
use crate::db::{Database, DatabaseBackend, DatabaseConfig};

async fn sqlite_database() -> Database {
    let config = DatabaseConfig::from_url("sqlite::memory:").unwrap();
    let database = Database::connect(&config).await.unwrap();
    database.migrate().await.unwrap();
    database
}

async fn repositories() -> (
    Database,
    TenantRepository,
    AgentRepository,
    PrinterRepository,
    CommandRepository,
) {
    let database = sqlite_database().await;
    (
        database.clone(),
        TenantRepository::new(database.clone()),
        AgentRepository::new(database.clone()),
        PrinterRepository::new(database.clone()),
        CommandRepository::new(database),
    )
}

#[tokio::test]
async fn sqlite_migrations_create_phase_1_schema() {
    let database = sqlite_database().await;
    let Database::Sqlite(pool) = database else {
        panic!("expected SQLite database");
    };

    for table in ["tenants", "users", "agents", "printers", "commands"] {
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
}

#[tokio::test]
async fn tenant_create_list_and_count_work() {
    let (_, tenants, _, _, _) = repositories().await;

    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    assert_eq!(tenant.slug, "acme");
    assert_eq!(tenants.count().await.unwrap(), 1);

    let listed = tenants.list().await.unwrap();
    assert_eq!(listed, vec![tenant]);
}

#[tokio::test]
async fn duplicate_tenant_slug_is_rejected() {
    let (_, tenants, _, _, _) = repositories().await;

    tenants.create("acme", "Acme Labs").await.unwrap();
    let err = tenants.create("acme", "Acme Again").await.unwrap_err();

    assert!(matches!(err, RepositoryError::DuplicateTenantSlug));
}

#[tokio::test]
async fn agent_create_and_list_are_scoped_to_tenant() {
    let (_, tenants, agents, _, _) = repositories().await;
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
    let (_, tenants, agents, _, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();

    agents.create(tenant.id, "shop-floor").await.unwrap();
    let err = agents.create(tenant.id, "shop-floor").await.unwrap_err();

    assert!(matches!(err, RepositoryError::DuplicateAgentName));
}

#[tokio::test]
async fn missing_tenant_is_reported_for_agent_create_and_list() {
    let (_, _, agents, _, _) = repositories().await;
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
async fn summary_counts_include_printer_and_command_fixtures() {
    let (database, tenants, agents, printers, commands) = repositories().await;
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
    let (database, tenants, _, _, _) = repositories().await;

    assert_eq!(database.backend(), DatabaseBackend::Sqlite);
    tenants.create("acme", "Acme Labs").await.unwrap();
    assert_eq!(tenants.count().await.unwrap(), 1);
}

async fn postgres_database() -> Option<Database> {
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

async fn clear_postgres(database: &Database) {
    let Database::Postgres(pool) = database else {
        panic!("expected PostgreSQL database");
    };
    sqlx::query("TRUNCATE commands, printers, agents, users, tenants")
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn postgres_core_repository_behavior_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());

    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    insert_command_fixture(&database, tenant.id, agent.id, Some(&printer_id))
        .await
        .unwrap();

    assert_eq!(tenants.list().await.unwrap(), vec![tenant.clone()]);
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
}

#[tokio::test]
async fn postgres_records_survive_reconnect_when_configured() {
    let url = match std::env::var("PANDAR_TEST_POSTGRES_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
            return;
        }
    };
    let config = DatabaseConfig::from_url(&url).unwrap();
    let database = Database::connect(&config).await.unwrap();
    database.migrate().await.unwrap();
    clear_postgres(&database).await;

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
