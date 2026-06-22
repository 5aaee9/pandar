use anyhow::Context;
use pandar_core::{AgentId, TenantId};
use sqlx::Row;

use crate::{
    db::Database,
    repositories::{RepositoryError, RepositoryResult},
};

pub async fn verify_agent_owner(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> RepositoryResult<()> {
    let persisted_tenant_id = match database {
        Database::Sqlite(pool) => {
            let row = sqlx::query("SELECT tenant_id FROM agents WHERE id = ?1")
                .bind(agent_id.to_string())
                .fetch_optional(pool)
                .await
                .context("failed to verify SQLite command agent ownership")?;
            row.map(|row| row.get::<String, _>("tenant_id"))
        }
        Database::Postgres(pool) => {
            let row = sqlx::query("SELECT tenant_id FROM agents WHERE id = $1")
                .bind(agent_id.to_string())
                .fetch_optional(pool)
                .await
                .context("failed to verify PostgreSQL command agent ownership")?;
            row.map(|row| row.get::<String, _>("tenant_id"))
        }
    };

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
    let serial_number = match database {
        Database::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT serial_number FROM printers WHERE id = ?1 AND tenant_id = ?2 AND agent_id = ?3",
            )
            .bind(printer_id)
            .bind(tenant_id.to_string())
            .bind(agent_id.to_string())
            .fetch_optional(pool)
            .await
            .context("failed to verify SQLite command printer ownership")?;
            row.map(|row| row.get::<String, _>("serial_number"))
        }
        Database::Postgres(pool) => {
            let row = sqlx::query(
                "SELECT serial_number FROM printers WHERE id = $1 AND tenant_id = $2 AND agent_id = $3",
            )
            .bind(printer_id)
            .bind(tenant_id.to_string())
            .bind(agent_id.to_string())
            .fetch_optional(pool)
            .await
            .context("failed to verify PostgreSQL command printer ownership")?;
            row.map(|row| row.get::<String, _>("serial_number"))
        }
    };

    serial_number.ok_or(RepositoryError::MissingPrinter)
}
