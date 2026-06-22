mod adapters;
mod agents;
mod audit;
mod auth;
mod commands;
mod jobs;
mod materials;
mod printers;
mod tenants;

pub use agents::AgentRepository;
pub use audit::{AuditEvent, AuditEventRepository, RecordAuditEvent};
pub use auth::{ApiToken, AuthRepository, AuthenticatedUser, User, UserIdentity, UserRole};
pub use commands::{
    CommandRepository, DiagnosePrinterPayload, DiscoverPrintersPayload, PrintProjectFilePayload,
};
pub use jobs::{
    AppliedPrintReport, ApplyPrintReport, CreatePrintJob, JobRepository, JobWithArtifact,
    PrintReportDiagnostic,
};
pub use materials::{MaterialPatchInput, MaterialRepository, MaterialSnapshot};
pub use printers::{PrinterRepository, PrinterSnapshotUpsert};
pub use tenants::TenantRepository;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("tenant slug already exists")]
    DuplicateTenantSlug,
    #[error("agent name already exists for tenant")]
    DuplicateAgentName,
    #[error("api token name already exists for tenant")]
    DuplicateApiTokenName,
    #[error("api token hash already exists")]
    DuplicateApiTokenHash,
    #[error("user email already exists for tenant")]
    DuplicateUserEmail,
    #[error("external identity already exists for tenant")]
    DuplicateExternalIdentity,
    #[error("external identity provider already linked to user")]
    DuplicateUserExternalIdentity,
    #[error("tenant not found")]
    MissingTenant,
    #[error("user not found")]
    MissingUser,
    #[error("api token not found")]
    MissingApiToken,
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
    #[error("invalid persisted print status: {0}")]
    InvalidPersistedPrintStatus(String),
    #[error("invalid persisted user role: {0}")]
    InvalidPersistedUserRole(String),
    #[error(transparent)]
    Database(#[from] anyhow::Error),
}

pub type RepositoryResult<T> = Result<T, RepositoryError>;

#[cfg(test)]
mod tests;

pub fn is_sea_orm_unique_violation(
    err: &sea_orm::DbErr,
    sqlite_name: &str,
    postgres_name: &str,
) -> bool {
    if let Some(sea_orm::SqlErr::UniqueConstraintViolation(message)) = err.sql_err()
        && (message.contains(sqlite_name) || message.contains(postgres_name))
    {
        return true;
    }

    let message = err.to_string();
    message.contains(sqlite_name) || message.contains(postgres_name)
}

pub fn is_sea_orm_foreign_key_violation(err: &sea_orm::DbErr) -> bool {
    if matches!(
        err.sql_err(),
        Some(sea_orm::SqlErr::ForeignKeyConstraintViolation(_))
    ) {
        return true;
    }

    let message = err.to_string();
    message.contains("23503") || message.contains("FOREIGN KEY constraint failed")
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
