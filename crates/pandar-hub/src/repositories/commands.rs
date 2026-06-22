use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandRecord, CommandStatus, TenantId};
use serde::{Deserialize, Serialize};

mod audit;
pub mod inserts;
mod ownership;
pub(crate) mod rows;
mod transitions;

use inserts::InsertCommand;
use rows::command_from_row;

use crate::{
    db::Database,
    repositories::{RepositoryError, RepositoryResult},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrintProjectFilePayload {
    pub job_id: String,
    pub artifact_id: String,
    pub printer_id: String,
    pub serial_number: String,
    pub filename: String,
    pub storage_path: String,
    pub size_bytes: u64,
    pub plate_id: u32,
    pub use_ams: bool,
    pub flow_cali: bool,
    pub timelapse: bool,
}

#[derive(Debug, Clone)]
pub struct CommandRepository {
    database: Database,
}

impl CommandRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn count(&self) -> RepositoryResult<i64> {
        let count = match &self.database {
            Database::Sqlite(pool) => {
                sqlx::query_scalar("SELECT COUNT(*) FROM commands")
                    .fetch_one(pool)
                    .await
            }
            Database::Postgres(pool) => {
                sqlx::query_scalar("SELECT COUNT(*) FROM commands")
                    .fetch_one(pool)
                    .await
            }
        }
        .context("failed to count commands")?;

        Ok(count)
    }

    pub async fn enqueue_refresh_printers(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<CommandRecord> {
        ownership::verify_agent_owner(&self.database, tenant_id, agent_id).await?;

        let id = CommandId::new();
        let now = pandar_core::created_at_now();
        inserts::insert(
            &self.database,
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

        self.get(id).await?.ok_or(RepositoryError::MissingCommand)
    }

    pub async fn enqueue_refresh_printers_with_audit(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        user_id: String,
    ) -> RepositoryResult<CommandRecord> {
        audit::enqueue_refresh_printers_with_audit(&self.database, tenant_id, agent_id, user_id)
            .await
    }

    pub async fn enqueue_print_project_file(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        printer_id: &str,
        payload: PrintProjectFilePayload,
    ) -> RepositoryResult<CommandRecord> {
        ownership::verify_agent_owner(&self.database, tenant_id, agent_id).await?;
        ownership::printer_serial_for_agent(&self.database, tenant_id, agent_id, printer_id)
            .await?;

        let id = CommandId::new();
        let now = pandar_core::created_at_now();
        let payload_json =
            serde_json::to_string(&payload).context("failed to serialize print command payload")?;
        inserts::insert(
            &self.database,
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

        self.get(id).await?.ok_or(RepositoryError::MissingCommand)
    }

    pub async fn next_queued_for_agent(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<Option<CommandRecord>> {
        match &self.database {
            Database::Sqlite(pool) => {
                let row = sqlx::query(
                    "SELECT id, tenant_id, agent_id, printer_id, kind, status, payload_json, error, created_at, updated_at
                     FROM commands
                     WHERE tenant_id = ?1 AND agent_id = ?2 AND status = ?3
                     ORDER BY created_at ASC, id ASC
                     LIMIT 1",
                )
                .bind(tenant_id.to_string())
                .bind(agent_id.to_string())
                .bind(CommandStatus::Queued.as_str())
                .fetch_optional(pool)
                .await
                .context("failed to load next queued SQLite command")?;
                row.map(command_from_row).transpose()
            }
            Database::Postgres(pool) => {
                let row = sqlx::query(
                    "SELECT id, tenant_id, agent_id, printer_id, kind, status, payload_json, error, created_at, updated_at
                     FROM commands
                     WHERE tenant_id = $1 AND agent_id = $2 AND status = $3
                     ORDER BY created_at ASC, id ASC
                     LIMIT 1",
                )
                .bind(tenant_id.to_string())
                .bind(agent_id.to_string())
                .bind(CommandStatus::Queued.as_str())
                .fetch_optional(pool)
                .await
                .context("failed to load next queued PostgreSQL command")?;
                row.map(command_from_row).transpose()
            }
        }
    }

    pub async fn mark_sent(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<CommandRecord> {
        self.guard_transition(CommandTransition {
            command_id,
            tenant_id,
            agent_id,
            next_status: CommandStatus::Sent,
            error: None,
            allowed_statuses: &[CommandStatus::Queued],
            action: "send",
        })
        .await
    }

    pub async fn mark_acknowledged(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<CommandRecord> {
        self.guard_transition(CommandTransition {
            command_id,
            tenant_id,
            agent_id,
            next_status: CommandStatus::Acknowledged,
            error: None,
            allowed_statuses: &[CommandStatus::Sent],
            action: "acknowledge",
        })
        .await
    }

    pub async fn mark_succeeded(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<CommandRecord> {
        self.guard_terminal_transition(
            command_id,
            tenant_id,
            agent_id,
            CommandStatus::Succeeded,
            None,
            "succeed",
        )
        .await
    }

    pub async fn mark_failed(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
        error: impl Into<String>,
    ) -> RepositoryResult<CommandRecord> {
        self.guard_terminal_transition(
            command_id,
            tenant_id,
            agent_id,
            CommandStatus::Failed,
            Some(error.into()),
            "fail",
        )
        .await
    }

    async fn guard_transition(
        &self,
        transition: CommandTransition<'_>,
    ) -> RepositoryResult<CommandRecord> {
        let updated = transitions::update_status_if_current(
            &self.database,
            transition.command_id,
            transition.tenant_id,
            transition.agent_id,
            transition.next_status,
            transition.error,
            transition.allowed_statuses,
        )
        .await?;
        if updated {
            return self
                .get(transition.command_id)
                .await?
                .ok_or(RepositoryError::MissingCommand);
        }

        let command = self
            .load_owned(
                transition.command_id,
                transition.tenant_id,
                transition.agent_id,
            )
            .await?;
        if !transition.allowed_statuses.contains(&command.status) {
            return Err(invalid_transition(command.status, transition.action));
        }

        self.get(transition.command_id)
            .await?
            .ok_or(RepositoryError::MissingCommand)
    }

    async fn guard_terminal_transition(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
        terminal_status: CommandStatus,
        error: Option<String>,
        action: &'static str,
    ) -> RepositoryResult<CommandRecord> {
        let updated = transitions::update_status_if_current(
            &self.database,
            command_id,
            tenant_id,
            agent_id,
            terminal_status.clone(),
            error,
            &[CommandStatus::Sent, CommandStatus::Acknowledged],
        )
        .await?;
        let command = self.load_owned(command_id, tenant_id, agent_id).await?;

        if updated || command.status == terminal_status {
            return Ok(command);
        }

        Err(invalid_transition(command.status, action))
    }

    pub(crate) async fn load_owned(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<CommandRecord> {
        let command = self
            .get(command_id)
            .await?
            .ok_or(RepositoryError::MissingCommand)?;
        if command.tenant_id != tenant_id || command.agent_id != agent_id {
            return Err(RepositoryError::CommandOwnershipMismatch);
        }

        Ok(command)
    }

    async fn get(&self, command_id: CommandId) -> RepositoryResult<Option<CommandRecord>> {
        match &self.database {
            Database::Sqlite(pool) => {
                let row = sqlx::query(
                    "SELECT id, tenant_id, agent_id, printer_id, kind, status, payload_json, error, created_at, updated_at
                     FROM commands
                     WHERE id = ?1",
                )
                .bind(command_id.to_string())
                .fetch_optional(pool)
                .await
                .context("failed to load SQLite command")?;
                row.map(command_from_row).transpose()
            }
            Database::Postgres(pool) => {
                let row = sqlx::query(
                    "SELECT id, tenant_id, agent_id, printer_id, kind, status, payload_json, error, created_at, updated_at
                     FROM commands
                     WHERE id = $1",
                )
                .bind(command_id.to_string())
                .fetch_optional(pool)
                .await
                .context("failed to load PostgreSQL command")?;
                row.map(command_from_row).transpose()
            }
        }
    }
}

struct CommandTransition<'a> {
    command_id: CommandId,
    tenant_id: TenantId,
    agent_id: AgentId,
    next_status: CommandStatus,
    error: Option<String>,
    allowed_statuses: &'a [CommandStatus],
    action: &'static str,
}

fn invalid_transition(status: CommandStatus, action: &'static str) -> RepositoryError {
    RepositoryError::InvalidCommandTransition {
        from: status.as_str().to_string(),
        action,
    }
}
