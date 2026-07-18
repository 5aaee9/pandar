use anyhow::Context;
use pandar_core::{AgentId, AgentStatus, CommandRecord, CommandStatus, TenantId};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseTransaction, EntityTrait, QueryFilter, QuerySelect,
};

use super::persist_printer_operation_tx;
use crate::{
    db::Database,
    entities::{agents, commands, printers},
    repositories::{
        AuditActor, PrintErrorAction, PrinterOperationKind, PrinterOperationPayload,
        RepositoryError, RepositoryResult, begin_current_agent_transaction,
        commands::{CommandRepository, ownership},
    },
    routes::printer_operations::plate_mismatch::supports,
};

#[derive(Debug, Clone)]
pub struct WebPrintErrorRecovery {
    pub action: PrintErrorAction,
    pub error_generation: u64,
    pub expected_agent_id: AgentId,
    pub expected_session_id: String,
}

#[derive(Debug, Clone)]
pub struct PersistedLivePrinterOperation {
    pub command: CommandRecord,
    pub serial_number: String,
    pub operation: PrinterOperationKind,
}

impl CommandRepository {
    pub async fn create_web_print_error_sent_with_audit(
        &self,
        tenant_id: TenantId,
        printer_id: &str,
        input: WebPrintErrorRecovery,
        actor: AuditActor,
    ) -> RepositoryResult<PersistedLivePrinterOperation> {
        create_web_print_error_sent_with_audit(&self.database, tenant_id, printer_id, input, actor)
            .await
    }
}

async fn create_web_print_error_sent_with_audit(
    database: &Database,
    tenant_id: TenantId,
    printer_id: &str,
    input: WebPrintErrorRecovery,
    actor: AuditActor,
) -> RepositoryResult<PersistedLivePrinterOperation> {
    #[cfg(test)]
    super::ownership_pause::wait(printer_id).await;

    let tx = match begin_current_agent_transaction(
        database,
        tenant_id,
        input.expected_agent_id,
        &input.expected_session_id,
    )
    .await
    {
        Ok(tx) => tx,
        Err(RepositoryError::AgentSessionNotCurrent | RepositoryError::MissingAgent) => {
            return Err(RepositoryError::PrinterControlUnavailable);
        }
        Err(error) => return Err(error),
    };
    validate_agent(&tx, tenant_id, &input).await?;
    let printer = locked_printer(&tx, tenant_id, printer_id, input.expected_agent_id).await?;
    let print_error = validate_printer(&printer, &input)?;
    ensure_no_inflight_native_recovery(&tx, tenant_id, printer_id).await?;

    let operation = PrinterOperationKind::HandlePrintError {
        error_action: input.action,
        print_error,
        printer_job_id: printer.print_job_id.clone().unwrap_or_default(),
        sequence_id: 0,
    };
    let command_printer = ownership::CommandPrinter {
        id: printer.id.clone(),
        agent_id: input.expected_agent_id,
        serial_number: printer.serial_number.clone(),
        model: printer.model.clone(),
    };
    let command_id = persist_printer_operation_tx(
        &tx,
        tenant_id,
        &command_printer,
        operation.clone(),
        actor,
        CommandStatus::Sent,
    )
    .await?;
    tx.commit()
        .await
        .context("failed to commit Web print error recovery command audit transaction")?;
    let command = super::super::get_command(database, command_id)
        .await?
        .ok_or(RepositoryError::MissingCommand)?;

    Ok(PersistedLivePrinterOperation {
        command,
        serial_number: printer.serial_number,
        operation,
    })
}

async fn validate_agent(
    tx: &DatabaseTransaction,
    tenant_id: TenantId,
    input: &WebPrintErrorRecovery,
) -> RepositoryResult<()> {
    let agent = agents::Entity::find_by_id(input.expected_agent_id.to_string())
        .one(tx)
        .await
        .context("failed to reload locked Web recovery agent")?
        .ok_or(RepositoryError::PrinterControlUnavailable)?;
    if agent.tenant_id != tenant_id.to_string()
        || agent.status != AgentStatus::Online.as_str()
        || agent.current_session_id.as_deref() != Some(&input.expected_session_id)
    {
        return Err(RepositoryError::PrinterControlUnavailable);
    }
    Ok(())
}

async fn locked_printer(
    tx: &DatabaseTransaction,
    tenant_id: TenantId,
    printer_id: &str,
    expected_agent_id: AgentId,
) -> RepositoryResult<printers::Model> {
    let query = printers::Entity::find_by_id(printer_id)
        .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
        .filter(printers::Column::AgentId.eq(expected_agent_id.to_string()));
    match tx.get_database_backend() {
        sea_orm::DatabaseBackend::Postgres => query.lock_exclusive().one(tx).await,
        _ => query.one(tx).await,
    }
    .context("failed to lock Web recovery printer")?
    .ok_or(RepositoryError::PrinterControlUnavailable)
}

fn validate_printer(
    printer: &printers::Model,
    input: &WebPrintErrorRecovery,
) -> RepositoryResult<u32> {
    let print_error = printer
        .print_error
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(RepositoryError::PrinterControlUnavailable)?;
    let matching_generation = i64::try_from(input.error_generation)
        .ok()
        .is_some_and(|generation| generation == printer.print_error_generation);
    let active_native_state = matches!(
        printer.print_gcode_state.as_deref(),
        Some("PREPARE" | "SLICING" | "RUNNING" | "PAUSE")
    );
    let inactive_coarse_state = ["IDLE", "OFFLINE", "FAILED"]
        .iter()
        .any(|state| printer.status.eq_ignore_ascii_case(state));
    if !matching_generation
        || printer.print_error_task_generation != Some(printer.print_task_generation)
        || printer.print_error_received_at.is_none()
        || printer.print_error_session_id.as_deref() != Some(&input.expected_session_id)
        || !active_native_state
        || inactive_coarse_state
        || !supports(&printer.serial_number, print_error, input.action)
    {
        return Err(RepositoryError::PrinterControlUnavailable);
    }

    if matches!(
        input.action,
        PrintErrorAction::Resume | PrintErrorAction::Ignore
    ) {
        let job_state = printer
            .print_job_attr
            .map(u64::try_from)
            .transpose()
            .context("failed to read locked Web recovery job attr")?
            .map(|job_attr| (job_attr >> 4) & 0x0f);
        if !matches!(job_state, Some(0 | 1)) {
            return Err(RepositoryError::PrinterControlUnavailable);
        }
    }
    Ok(print_error)
}

pub(super) async fn ensure_no_inflight_native_recovery(
    tx: &DatabaseTransaction,
    tenant_id: TenantId,
    printer_id: &str,
) -> RepositoryResult<()> {
    let candidates = commands::Entity::find()
        .filter(commands::Column::TenantId.eq(tenant_id.to_string()))
        .filter(commands::Column::PrinterId.eq(printer_id))
        .filter(commands::Column::Kind.eq("printer_operation"))
        .filter(commands::Column::Status.is_in([
            CommandStatus::Sent.as_str(),
            CommandStatus::Acknowledged.as_str(),
        ]))
        .all(tx)
        .await
        .context("failed to load in-flight native printer recovery commands")?;
    for command in candidates {
        let payload: PrinterOperationPayload = serde_json::from_str(&command.payload_json)
            .context("failed to deserialize in-flight printer operation payload")?;
        if matches!(
            payload.operation,
            PrinterOperationKind::HandlePrintError { .. }
        ) {
            return Err(RepositoryError::PrinterControlUnavailable);
        }
    }
    Ok(())
}
