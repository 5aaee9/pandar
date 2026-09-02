use anyhow::Context;
use pandar_core::{AgentId, TenantId};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

use crate::db::ConnectionDialectExt;
use crate::{
    db::Database,
    entities::{agents, printers},
    repositories::{RepositoryError, RepositoryResult},
};

pub struct CommandPrinter {
    pub id: String,
    pub agent_id: AgentId,
    pub serial_number: String,
    pub model: Option<String>,
}

pub async fn verify_agent_owner(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> RepositoryResult<()> {
    let persisted_tenant_id = agents::Entity::find_by_id(agent_id.to_string())
        .one(&database.sea_orm_connection())
        .await
        .context("failed to verify command agent ownership")?
        .map(|agent| agent.tenant_id);

    let Some(persisted_tenant_id) = persisted_tenant_id else {
        return Err(RepositoryError::MissingAgent);
    };

    if persisted_tenant_id != tenant_id.to_string() {
        return Err(RepositoryError::CommandOwnershipMismatch);
    }

    Ok(())
}

pub async fn lock_agent_owner_on<C>(
    connection: &C,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    let query = agents::Entity::find_by_id(agent_id.to_string());
    let agent = connection
        .lock_for_update(query)
        .one(connection)
        .await
        .context("failed to lock command agent ownership")?
        .ok_or(RepositoryError::MissingAgent)?;
    if agent.tenant_id != tenant_id.to_string() {
        return Err(RepositoryError::CommandOwnershipMismatch);
    }
    Ok(())
}

#[cfg(test)]
pub async fn printer_serial_for_agent(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
    printer_id: &str,
) -> RepositoryResult<String> {
    let serial_number = printers::Entity::find_by_id(printer_id)
        .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
        .filter(printers::Column::AgentId.eq(agent_id.to_string()))
        .one(&database.sea_orm_connection())
        .await
        .context("failed to verify command printer ownership")?
        .map(|printer| printer.serial_number);

    serial_number.ok_or(RepositoryError::MissingPrinter)
}

pub async fn printer_for_tenant(
    database: &Database,
    tenant_id: TenantId,
    printer_id: &str,
) -> RepositoryResult<CommandPrinter> {
    let printer = printers::Entity::find_by_id(printer_id)
        .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
        .one(&database.sea_orm_connection())
        .await
        .context("failed to load command printer")?
        .ok_or(RepositoryError::MissingPrinter)?;

    Ok(CommandPrinter {
        id: printer.id,
        agent_id: AgentId::parse(&printer.agent_id).map_err(|err| {
            RepositoryError::Database(
                anyhow::Error::new(err).context("failed to parse command printer agent id"),
            )
        })?,
        serial_number: printer.serial_number,
        model: printer.model,
    })
}

pub async fn locked_printer_for_expected_agent<C>(
    connection: &C,
    tenant_id: TenantId,
    printer_id: &str,
    expected_agent_id: AgentId,
) -> RepositoryResult<CommandPrinter>
where
    C: ConnectionTrait,
{
    let query = printers::Entity::find_by_id(printer_id)
        .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
        .filter(printers::Column::AgentId.eq(expected_agent_id.to_string()));
    let printer = connection
        .lock_for_update(query)
        .one(connection)
        .await
        .context("failed to lock command printer ownership")?
        .ok_or(RepositoryError::PrinterControlUnavailable)?;

    Ok(CommandPrinter {
        id: printer.id,
        agent_id: AgentId::parse(&printer.agent_id).map_err(|err| {
            RepositoryError::Database(
                anyhow::Error::new(err).context("failed to parse command printer agent id"),
            )
        })?,
        serial_number: printer.serial_number,
        model: printer.model,
    })
}
