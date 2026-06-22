use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandRecord, TenantId};
use sea_orm::EntityTrait;

use super::{
    DiagnosePrinterPayload, DiscoverPrintersPayload, PrintProjectFilePayload, command_from_model,
    inserts, inserts::InsertCommand, ownership,
};
use crate::{
    db::Database,
    repositories::{RepositoryError, RepositoryResult},
};

pub async fn refresh_printers(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> RepositoryResult<CommandRecord> {
    ownership::verify_agent_owner(database, tenant_id, agent_id).await?;

    let id = CommandId::new();
    let now = pandar_core::created_at_now();
    inserts::insert(
        &database.sea_orm_connection(),
        InsertCommand {
            id,
            tenant_id,
            agent_id,
            printer_id: None,
            kind: "refresh_printers",
            payload_json: "{}",
            created_at: &now,
        },
    )
    .await
    .context("failed to enqueue refresh printers command")?;

    load(database, id)
        .await?
        .ok_or(RepositoryError::MissingCommand)
}

pub async fn print_project_file(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
    printer_id: &str,
    payload: PrintProjectFilePayload,
) -> RepositoryResult<CommandRecord> {
    ownership::verify_agent_owner(database, tenant_id, agent_id).await?;
    ownership::printer_serial_for_agent(database, tenant_id, agent_id, printer_id).await?;

    let id = CommandId::new();
    let now = pandar_core::created_at_now();
    let payload_json =
        serde_json::to_string(&payload).context("failed to serialize print command payload")?;
    inserts::insert(
        &database.sea_orm_connection(),
        InsertCommand {
            id,
            tenant_id,
            agent_id,
            printer_id: Some(printer_id),
            kind: "print_project_file",
            payload_json: &payload_json,
            created_at: &now,
        },
    )
    .await
    .context("failed to enqueue print project file command")?;

    load(database, id)
        .await?
        .ok_or(RepositoryError::MissingCommand)
}

pub async fn discover_printers(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
    payload: DiscoverPrintersPayload,
) -> RepositoryResult<CommandRecord> {
    ownership::verify_agent_owner(database, tenant_id, agent_id).await?;

    let id = CommandId::new();
    let now = pandar_core::created_at_now();
    let payload_json = serde_json::to_string(&payload)
        .context("failed to serialize discover printers command payload")?;
    inserts::insert(
        &database.sea_orm_connection(),
        InsertCommand {
            id,
            tenant_id,
            agent_id,
            printer_id: None,
            kind: "discover_printers",
            payload_json: &payload_json,
            created_at: &now,
        },
    )
    .await
    .context("failed to enqueue discover printers command")?;

    load(database, id)
        .await?
        .ok_or(RepositoryError::MissingCommand)
}

pub async fn diagnose_printer(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
    payload: DiagnosePrinterPayload,
) -> RepositoryResult<CommandRecord> {
    ownership::verify_agent_owner(database, tenant_id, agent_id).await?;

    let id = CommandId::new();
    let now = pandar_core::created_at_now();
    let payload_json = serde_json::to_string(&payload)
        .context("failed to serialize diagnose printer command payload")?;
    inserts::insert(
        &database.sea_orm_connection(),
        InsertCommand {
            id,
            tenant_id,
            agent_id,
            printer_id: None,
            kind: "diagnose_printer",
            payload_json: &payload_json,
            created_at: &now,
        },
    )
    .await
    .context("failed to enqueue diagnose printer command")?;

    load(database, id)
        .await?
        .ok_or(RepositoryError::MissingCommand)
}

async fn load(
    database: &Database,
    command_id: CommandId,
) -> RepositoryResult<Option<CommandRecord>> {
    crate::entities::commands::Entity::find_by_id(command_id.to_string())
        .one(&database.sea_orm_connection())
        .await
        .context("failed to load command")?
        .map(command_from_model)
        .transpose()
}
