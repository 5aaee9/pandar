mod commands;
mod phase1;
mod postgres;

use pandar_core::{AgentId, CommandId, TenantId};

use super::{
    AgentRepository, CommandRepository, PrinterRepository, RepositoryError, TenantRepository,
};
use crate::db::{Database, DatabaseConfig};

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

async fn command_repositories() -> (
    TenantRepository,
    AgentRepository,
    CommandRepository,
    pandar_core::Tenant,
    pandar_core::Agent,
) {
    let (_, tenants, agents, _, commands) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    (tenants, agents, commands, tenant, agent)
}

async fn enqueue_sent(
    commands: &CommandRepository,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> CommandId {
    let command = commands
        .enqueue_refresh_printers(tenant_id, agent_id)
        .await
        .unwrap();
    commands
        .mark_sent(command.id, tenant_id, agent_id)
        .await
        .unwrap();
    command.id
}
