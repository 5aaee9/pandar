use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandRecord, CommandStatus, TenantId};
use sea_orm::{
    DatabaseConnection, DatabaseTransaction, SqliteTransactionMode, TransactionOptions,
    TransactionTrait,
};

use super::get_command;
use crate::{
    db::Database,
    repositories::{
        AuditActor, RepositoryError, RepositoryResult,
        audit::{insert_audit_event_tx, record_audit_event},
        commands::{
            CommandRepository, PrinterOperationKind, PrinterOperationPayload,
            inserts::{self, InsertCommand},
            operation_audit_metadata, ownership, validate_printer_operation,
        },
    },
};

mod recovery;

pub use recovery::{PersistedLivePrinterOperation, WebPrintErrorRecovery};

impl CommandRepository {
    pub async fn create_printer_operation_sent_with_audit(
        &self,
        tenant_id: TenantId,
        printer_id: &str,
        expected_agent_id: AgentId,
        operation: PrinterOperationKind,
        actor: AuditActor,
    ) -> RepositoryResult<CommandRecord> {
        create_printer_operation_sent_with_audit(
            &self.database,
            tenant_id,
            printer_id,
            expected_agent_id,
            operation,
            actor,
        )
        .await
    }
}

pub(in crate::repositories::commands) async fn enqueue_printer_operation_with_audit(
    database: &Database,
    tenant_id: TenantId,
    printer_id: &str,
    operation: PrinterOperationKind,
    actor: AuditActor,
) -> RepositoryResult<CommandRecord> {
    if matches!(&operation, PrinterOperationKind::HandlePrintError { .. }) {
        return Err(RepositoryError::InvalidPrinterControl);
    }
    validate_printer_operation(&operation)?;
    let printer = ownership::printer_for_tenant(database, tenant_id, printer_id).await?;
    ownership::verify_agent_owner(database, tenant_id, printer.agent_id).await?;
    if !pandar_core::compatibility::live_controls_supported(printer.model.as_deref()) {
        return Err(RepositoryError::PrinterControlUnavailable);
    }

    persist_printer_operation(
        database,
        tenant_id,
        printer,
        operation,
        actor,
        CommandStatus::Queued,
        "printer operation",
    )
    .await
}

pub(in crate::repositories::commands) async fn create_printer_operation_sent_with_audit(
    database: &Database,
    tenant_id: TenantId,
    printer_id: &str,
    expected_agent_id: AgentId,
    operation: PrinterOperationKind,
    actor: AuditActor,
) -> RepositoryResult<CommandRecord> {
    validate_printer_operation(&operation)?;
    if !operation.required_device_features().is_empty() {
        return Err(RepositoryError::InvalidPrinterControl);
    }

    #[cfg(test)]
    ownership_pause::wait(printer_id).await;

    let connection = database.sea_orm_connection();
    let tx = begin_live_printer_operation_transaction(&connection)
        .await
        .context("failed to begin sent printer operation command audit transaction")?;
    ownership::lock_agent_owner_on(&tx, tenant_id, expected_agent_id).await?;
    let printer =
        ownership::locked_printer_for_expected_agent(&tx, tenant_id, printer_id, expected_agent_id)
            .await?;
    if matches!(&operation, PrinterOperationKind::HandlePrintError { .. }) {
        recovery::ensure_no_inflight_native_recovery(&tx, tenant_id, printer_id).await?;
    }
    let command_id = persist_printer_operation_tx(
        &tx,
        tenant_id,
        &printer,
        operation,
        actor,
        CommandStatus::Sent,
    )
    .await?;
    tx.commit()
        .await
        .context("failed to commit sent printer operation command audit transaction")?;

    get_command(database, command_id)
        .await?
        .ok_or(RepositoryError::MissingCommand)
}

async fn persist_printer_operation(
    database: &Database,
    tenant_id: TenantId,
    printer: ownership::CommandPrinter,
    operation: PrinterOperationKind,
    actor: AuditActor,
    status: CommandStatus,
    context_label: &'static str,
) -> RepositoryResult<CommandRecord> {
    let connection = database.sea_orm_connection();
    let tx = connection
        .begin()
        .await
        .with_context(|| format!("failed to begin {context_label} command audit transaction"))?;
    let id =
        persist_printer_operation_tx(&tx, tenant_id, &printer, operation, actor, status).await?;
    tx.commit()
        .await
        .with_context(|| format!("failed to commit {context_label} command audit transaction"))?;

    get_command(database, id)
        .await?
        .ok_or(RepositoryError::MissingCommand)
}

async fn persist_printer_operation_tx(
    tx: &DatabaseTransaction,
    tenant_id: TenantId,
    printer: &ownership::CommandPrinter,
    operation: PrinterOperationKind,
    actor: AuditActor,
    status: CommandStatus,
) -> RepositoryResult<CommandId> {
    let payload = PrinterOperationPayload {
        printer_id: printer.id.clone(),
        serial_number: printer.serial_number.clone(),
        operation,
    };
    let payload_json = serde_json::to_string(&payload)
        .context("failed to serialize printer operation command payload")?;
    let id = CommandId::new();
    let now = pandar_core::created_at_now();
    inserts::insert_with_status(
        tx,
        InsertCommand {
            id,
            tenant_id,
            agent_id: printer.agent_id,
            printer_id: Some(&printer.id),
            kind: "printer_operation",
            payload_json: &payload_json,
            created_at: &now,
        },
        status,
    )
    .await?;
    insert_audit_event_tx(
        tx,
        &printer_operation_audit_event(tenant_id, printer, &payload.operation, actor),
    )
    .await?;
    Ok(id)
}

async fn begin_live_printer_operation_transaction(
    connection: &DatabaseConnection,
) -> Result<DatabaseTransaction, sea_orm::DbErr> {
    match connection.get_database_backend() {
        sea_orm::DatabaseBackend::Sqlite => {
            connection
                .begin_with_options(TransactionOptions {
                    sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
                    ..Default::default()
                })
                .await
        }
        _ => connection.begin().await,
    }
}

fn printer_operation_audit_event(
    tenant_id: TenantId,
    printer: &ownership::CommandPrinter,
    operation: &PrinterOperationKind,
    actor: AuditActor,
) -> crate::repositories::AuditEvent {
    record_audit_event(
        tenant_id,
        actor,
        "printer.dispatch_control",
        "printer",
        Some(printer.id.clone()),
        operation_audit_metadata(
            printer.agent_id.to_string(),
            printer.serial_number.clone(),
            operation,
        ),
    )
}

#[cfg(test)]
pub(crate) mod ownership_pause;
