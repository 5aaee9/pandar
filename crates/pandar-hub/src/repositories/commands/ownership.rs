use anyhow::Context;
use pandar_core::{AgentId, TenantId};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::{
    db::Database,
    entities::{agents, printers},
    repositories::{RepositoryError, RepositoryResult},
};

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
