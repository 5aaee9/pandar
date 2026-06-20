use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandRecord, CommandStatus, TenantId};
use sqlx::Row;

mod rows;
mod transitions;

use rows::command_from_row;

use crate::{
    db::Database,
    repositories::{RepositoryError, RepositoryResult},
};

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
        self.verify_agent_owner(tenant_id, agent_id).await?;

        let id = CommandId::new();
        let now = pandar_core::created_at_now();
        let result = match &self.database {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "INSERT INTO commands (id, tenant_id, agent_id, printer_id, kind, status, payload_json, error, created_at, updated_at)
                     VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, NULL, ?7, ?8)",
                )
                .bind(id.to_string())
                .bind(tenant_id.to_string())
                .bind(agent_id.to_string())
                .bind("refresh_printers")
                .bind(CommandStatus::Queued.as_str())
                .bind("{}")
                .bind(&now)
                .bind(&now)
                .execute(pool)
                .await
                .map(|_| ())
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO commands (id, tenant_id, agent_id, printer_id, kind, status, payload_json, error, created_at, updated_at)
                     VALUES ($1, $2, $3, NULL, $4, $5, $6, NULL, $7, $8)",
                )
                .bind(id.to_string())
                .bind(tenant_id.to_string())
                .bind(agent_id.to_string())
                .bind("refresh_printers")
                .bind(CommandStatus::Queued.as_str())
                .bind("{}")
                .bind(&now)
                .bind(&now)
                .execute(pool)
                .await
                .map(|_| ())
            }
        };
        result.context("failed to enqueue refresh printers command")?;

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

    async fn load_owned(
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

    async fn verify_agent_owner(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<()> {
        let persisted_tenant_id = match &self.database {
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
