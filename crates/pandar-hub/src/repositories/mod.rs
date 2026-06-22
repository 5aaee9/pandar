mod agents;
mod commands;
mod jobs;
mod printers;
mod tenants;

pub use agents::AgentRepository;
pub use commands::{CommandRepository, PrintProjectFilePayload};
pub use jobs::{CreatePrintJob, JobRepository, JobWithArtifact};
pub use printers::{PrinterRepository, PrinterSnapshotUpsert};
pub use tenants::TenantRepository;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("tenant slug already exists")]
    DuplicateTenantSlug,
    #[error("agent name already exists for tenant")]
    DuplicateAgentName,
    #[error("tenant not found")]
    MissingTenant,
    #[error("agent not found")]
    MissingAgent,
    #[error("printer not found")]
    MissingPrinter,
    #[error("command not found")]
    MissingCommand,
    #[error("job not found")]
    MissingJob,
    #[error("command belongs to a different tenant or agent")]
    CommandOwnershipMismatch,
    #[error("cannot {action} command from {from}")]
    InvalidCommandTransition { from: String, action: &'static str },
    #[error("invalid persisted agent status: {0}")]
    InvalidPersistedStatus(String),
    #[error("invalid persisted command status: {0}")]
    InvalidPersistedCommandStatus(String),
    #[error("invalid persisted job status: {0}")]
    InvalidPersistedJobStatus(String),
    #[error(transparent)]
    Database(#[from] anyhow::Error),
}

pub type RepositoryResult<T> = Result<T, RepositoryError>;

#[cfg(test)]
mod tests;

fn is_unique_violation(err: &sqlx::Error, sqlite_name: &str, postgres_name: &str) -> bool {
    let Some(db_err) = err.as_database_error() else {
        return false;
    };

    if db_err.constraint() == Some(sqlite_name) || db_err.constraint() == Some(postgres_name) {
        return true;
    }

    db_err.message().contains(sqlite_name) || db_err.message().contains(postgres_name)
}

fn is_foreign_key_violation(err: &sqlx::Error) -> bool {
    let Some(db_err) = err.as_database_error() else {
        return false;
    };

    db_err.code().as_deref() == Some("23503")
        || db_err.message().contains("FOREIGN KEY constraint failed")
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use anyhow::Context;
    use pandar_core::{AgentId, TenantId};

    use crate::db::Database;

    pub(crate) async fn insert_printer_fixture(
        database: &Database,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> anyhow::Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        match database {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "INSERT INTO printers (id, tenant_id, agent_id, serial_number, name, model, status, last_seen_at, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?7)",
                )
                .bind(&id)
                .bind(tenant_id.to_string())
                .bind(agent_id.to_string())
                .bind(format!("serial-{id}"))
                .bind("Fixture Printer")
                .bind("offline")
                .bind("2026-06-20T00:00:00Z")
                .execute(pool)
                .await
                .context("failed to insert SQLite printer fixture")?;
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO printers (id, tenant_id, agent_id, serial_number, name, model, status, last_seen_at, created_at)
                     VALUES ($1, $2, $3, $4, $5, NULL, $6, $7, $7)",
                )
                .bind(&id)
                .bind(tenant_id.to_string())
                .bind(agent_id.to_string())
                .bind(format!("serial-{id}"))
                .bind("Fixture Printer")
                .bind("offline")
                .bind("2026-06-20T00:00:00Z")
                .execute(pool)
                .await
                .context("failed to insert PostgreSQL printer fixture")?;
            }
        }

        Ok(id)
    }

    pub(crate) async fn insert_command_fixture(
        database: &Database,
        tenant_id: TenantId,
        agent_id: AgentId,
        printer_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let id = format!("command-{agent_id}");
        let now = "2026-06-20T00:00:00Z";
        match database {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "INSERT INTO commands (id, tenant_id, agent_id, printer_id, kind, status, payload_json, error, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9)",
                )
                .bind(id)
                .bind(tenant_id.to_string())
                .bind(agent_id.to_string())
                .bind(printer_id)
                .bind("sync")
                .bind("queued")
                .bind("{}")
                .bind(now)
                .bind(now)
                .execute(pool)
                .await
                .context("failed to insert SQLite command fixture")?;
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO commands (id, tenant_id, agent_id, printer_id, kind, status, payload_json, error, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, $8, $9)",
                )
                .bind(id)
                .bind(tenant_id.to_string())
                .bind(agent_id.to_string())
                .bind(printer_id)
                .bind("sync")
                .bind("queued")
                .bind("{}")
                .bind(now)
                .bind(now)
                .execute(pool)
                .await
                .context("failed to insert PostgreSQL command fixture")?;
            }
        }

        Ok(())
    }
}
