use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandRecord, CommandStatus, TenantId};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};

mod audit;
mod enqueue;
pub mod inserts;
mod ownership;
pub(crate) mod rows;
mod transitions;

use rows::command_from_model;

use crate::{
    db::Database,
    entities::commands,
    repositories::{AuditActor, RepositoryError, RepositoryResult},
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
    pub ams_mapping_json: Option<String>,
    pub ams_mapping2_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoverPrintersPayload {
    pub timeout_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosePrinterPayload {
    pub serial_number: String,
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
        let count = commands::Entity::find()
            .count(&self.database.sea_orm_connection())
            .await
            .context("failed to count commands")?;

        Ok(count.try_into().expect("command count should fit in i64"))
    }

    pub async fn enqueue_refresh_printers(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<CommandRecord> {
        enqueue::refresh_printers(&self.database, tenant_id, agent_id).await
    }

    pub async fn enqueue_refresh_printers_with_audit(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        actor: AuditActor,
    ) -> RepositoryResult<CommandRecord> {
        audit::enqueue_refresh_printers_with_audit(&self.database, tenant_id, agent_id, actor).await
    }

    pub async fn enqueue_discover_printers_with_audit(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        payload: DiscoverPrintersPayload,
        actor: AuditActor,
    ) -> RepositoryResult<CommandRecord> {
        audit::enqueue_discover_printers_with_audit(
            &self.database,
            tenant_id,
            agent_id,
            payload,
            actor,
        )
        .await
    }

    pub async fn enqueue_diagnose_printer_with_audit(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        payload: DiagnosePrinterPayload,
        actor: AuditActor,
    ) -> RepositoryResult<CommandRecord> {
        audit::enqueue_diagnose_printer_with_audit(
            &self.database,
            tenant_id,
            agent_id,
            payload,
            actor,
        )
        .await
    }

    pub async fn enqueue_print_project_file(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        printer_id: &str,
        payload: PrintProjectFilePayload,
    ) -> RepositoryResult<CommandRecord> {
        enqueue::print_project_file(&self.database, tenant_id, agent_id, printer_id, payload).await
    }

    pub async fn enqueue_discover_printers(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        payload: DiscoverPrintersPayload,
    ) -> RepositoryResult<CommandRecord> {
        enqueue::discover_printers(&self.database, tenant_id, agent_id, payload).await
    }

    pub async fn enqueue_diagnose_printer(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        payload: DiagnosePrinterPayload,
    ) -> RepositoryResult<CommandRecord> {
        enqueue::diagnose_printer(&self.database, tenant_id, agent_id, payload).await
    }

    pub async fn next_queued_for_agent(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<Option<CommandRecord>> {
        commands::Entity::find()
            .filter(commands::Column::TenantId.eq(tenant_id.to_string()))
            .filter(commands::Column::AgentId.eq(agent_id.to_string()))
            .filter(commands::Column::Status.eq(CommandStatus::Queued.as_str()))
            .order_by_asc(commands::Column::CreatedAt)
            .order_by_asc(commands::Column::Id)
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to load next queued command")?
            .map(command_from_model)
            .transpose()
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
        self.mark_succeeded_with_result(command_id, tenant_id, agent_id, None)
            .await
    }

    pub async fn mark_succeeded_with_result(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
        result_json: Option<String>,
    ) -> RepositoryResult<CommandRecord> {
        self.guard_terminal_transition(TerminalCommandTransition {
            command_id,
            tenant_id,
            agent_id,
            terminal_status: CommandStatus::Succeeded,
            error: None,
            result_json,
            action: "succeed",
        })
        .await
    }

    pub async fn mark_failed(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
        error: impl Into<String>,
    ) -> RepositoryResult<CommandRecord> {
        self.mark_failed_with_result(command_id, tenant_id, agent_id, error, None)
            .await
    }

    pub async fn mark_failed_with_result(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
        error: impl Into<String>,
        result_json: Option<String>,
    ) -> RepositoryResult<CommandRecord> {
        self.guard_terminal_transition(TerminalCommandTransition {
            command_id,
            tenant_id,
            agent_id,
            terminal_status: CommandStatus::Failed,
            error: Some(error.into()),
            result_json,
            action: "fail",
        })
        .await
    }

    async fn guard_transition(
        &self,
        transition: CommandTransition<'_>,
    ) -> RepositoryResult<CommandRecord> {
        let updated = transitions::update_status_if_current(
            &self.database,
            transitions::StatusTransition {
                command_id: transition.command_id,
                tenant_id: transition.tenant_id,
                agent_id: transition.agent_id,
                status: transition.next_status,
                error: transition.error,
                result_json: None,
                allowed_statuses: transition.allowed_statuses,
            },
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
        transition: TerminalCommandTransition,
    ) -> RepositoryResult<CommandRecord> {
        let updated = transitions::update_status_if_current(
            &self.database,
            transitions::StatusTransition {
                command_id: transition.command_id,
                tenant_id: transition.tenant_id,
                agent_id: transition.agent_id,
                status: transition.terminal_status.clone(),
                error: transition.error,
                result_json: transition.result_json,
                allowed_statuses: &[CommandStatus::Sent, CommandStatus::Acknowledged],
            },
        )
        .await?;
        let command = self
            .load_owned(
                transition.command_id,
                transition.tenant_id,
                transition.agent_id,
            )
            .await?;

        if updated || command.status == transition.terminal_status {
            return Ok(command);
        }

        Err(invalid_transition(command.status, transition.action))
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

    pub async fn get_for_tenant(
        &self,
        tenant_id: TenantId,
        command_id: CommandId,
    ) -> RepositoryResult<Option<CommandRecord>> {
        commands::Entity::find_by_id(command_id.to_string())
            .filter(commands::Column::TenantId.eq(tenant_id.to_string()))
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to load tenant command")?
            .map(command_from_model)
            .transpose()
    }

    async fn get(&self, command_id: CommandId) -> RepositoryResult<Option<CommandRecord>> {
        commands::Entity::find_by_id(command_id.to_string())
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to load command")?
            .map(command_from_model)
            .transpose()
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

struct TerminalCommandTransition {
    command_id: CommandId,
    tenant_id: TenantId,
    agent_id: AgentId,
    terminal_status: CommandStatus,
    error: Option<String>,
    result_json: Option<String>,
    action: &'static str,
}

fn invalid_transition(status: CommandStatus, action: &'static str) -> RepositoryError {
    RepositoryError::InvalidCommandTransition {
        from: status.as_str().to_string(),
        action,
    }
}
