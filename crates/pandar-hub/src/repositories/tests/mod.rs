mod auth;
mod cleanup;
mod command_results;
mod commands;
mod jobs;
mod materials;
mod phase1;
mod postgres;
mod postgres_commands;
mod postgres_persistence;
mod printers;
mod tenant_tokens;

use pandar_core::{AgentId, CommandId, TenantId};

use super::{
    AgentRepository, AuditEventRepository, AuthRepository, CommandRepository, JobRepository,
    MaterialRepository, PrinterRepository, PrinterSnapshotUpsert, RepositoryError,
    TenantRepository,
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
    JobRepository,
) {
    let database = sqlite_database().await;
    (
        database.clone(),
        TenantRepository::new(database.clone()),
        AgentRepository::new(database.clone()),
        PrinterRepository::new(database.clone()),
        CommandRepository::new(database.clone()),
        JobRepository::new(database),
    )
}

async fn command_repositories() -> (
    TenantRepository,
    AgentRepository,
    CommandRepository,
    pandar_core::Tenant,
    pandar_core::Agent,
) {
    let (_, tenants, agents, _, commands, _) = repositories().await;
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

async fn material_repositories() -> (
    Database,
    TenantRepository,
    AgentRepository,
    PrinterRepository,
    MaterialRepository,
) {
    let database = sqlite_database().await;
    (
        database.clone(),
        TenantRepository::new(database.clone()),
        AgentRepository::new(database.clone()),
        PrinterRepository::new(database.clone()),
        MaterialRepository::new(database),
    )
}
