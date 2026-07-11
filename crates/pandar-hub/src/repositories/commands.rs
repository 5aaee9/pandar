use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandRecord, CommandStatus, TenantId};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};

mod audit;
mod enqueue;
pub mod inserts;
mod operations;
mod ownership;
mod query;
pub(crate) mod rows;
mod transitions;
mod types;
use rows::command_from_model;
use transitions::{CommandTransition, TerminalCommandTransition, invalid_transition};
pub use types::{
    DiagnosePrinterPayload, DiscoverPrintersPayload, LinkPrinterPayload, PrintProjectFilePayload,
    RefreshPrinterMaterialsPayload,
};

pub use audit::{PersistedLivePrinterOperation, WebPrintErrorRecovery};
pub use operations::{
    PrintErrorAction, PrinterAxis, PrinterAxisMovement, PrinterOperationKind,
    PrinterOperationPayload, operation_audit_metadata, validate_printer_operation,
};

#[cfg(test)]
pub(crate) use audit::ownership_pause as printer_operation_ownership_pause;

use crate::{
    db::Database,
    entities::commands,
    repositories::{AuditActor, RepositoryError, RepositoryResult},
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

    pub async fn enqueue_printer_operation_with_audit(
        &self,
        tenant_id: TenantId,
        printer_id: &str,
        operation: PrinterOperationKind,
        actor: AuditActor,
    ) -> RepositoryResult<CommandRecord> {
        audit::enqueue_printer_operation_with_audit(
            &self.database,
            tenant_id,
            printer_id,
            operation,
            actor,
        )
        .await
    }

    pub async fn enqueue_refresh_printer_materials_with_audit(
        &self,
        tenant_id: TenantId,
        printer_id: &str,
        actor: AuditActor,
    ) -> RepositoryResult<CommandRecord> {
        audit::enqueue_refresh_printer_materials_with_audit(
            &self.database,
            tenant_id,
            printer_id,
            actor,
        )
        .await
    }

    pub async fn create_link_printer_sent_with_audit(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        payload: LinkPrinterPayload,
        actor: AuditActor,
    ) -> RepositoryResult<CommandRecord> {
        audit::create_link_printer_sent_with_audit(
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

    pub async fn fail_stale_unowned_live_commands(
        &self,
        now: &str,
        timeout: std::time::Duration,
        owned_command_ids: &[CommandId],
    ) -> RepositoryResult<u64> {
        transitions::fail_stale_unowned_live_commands(
            &self.database,
            now,
            timeout,
            owned_command_ids,
        )
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
}
