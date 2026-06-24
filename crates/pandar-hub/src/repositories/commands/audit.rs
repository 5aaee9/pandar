use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandRecord, TenantId};
use sea_orm::{EntityTrait, TransactionTrait};

use crate::{
    db::Database,
    repositories::{
        AuditActor, RepositoryError, RepositoryResult,
        audit::{insert_audit_event_tx, record_audit_event},
        commands::{
            DiagnosePrinterPayload, DiscoverPrintersPayload, PrinterOperationKind,
            PrinterOperationPayload,
            inserts::{self, InsertCommand},
            operation_audit_metadata, ownership,
            rows::command_from_model,
            validate_printer_operation,
        },
    },
};

pub async fn enqueue_refresh_printers_with_audit(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
    actor: AuditActor,
) -> RepositoryResult<CommandRecord> {
    ownership::verify_agent_owner(database, tenant_id, agent_id).await?;
    let id = CommandId::new();
    let now = pandar_core::created_at_now();
    let connection = database.sea_orm_connection();
    let tx = connection
        .begin()
        .await
        .context("failed to begin refresh command audit transaction")?;
    inserts::insert(
        &tx,
        insert_command(id, tenant_id, agent_id, "refresh_printers", "{}", &now),
    )
    .await?;
    let event = refresh_audit_event(tenant_id, agent_id, actor);
    insert_audit_event_tx(&tx, &event).await?;
    tx.commit()
        .await
        .context("failed to commit refresh command audit transaction")?;

    get_command(database, id)
        .await?
        .ok_or(RepositoryError::MissingCommand)
}

pub async fn enqueue_discover_printers_with_audit(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
    payload: DiscoverPrintersPayload,
    actor: AuditActor,
) -> RepositoryResult<CommandRecord> {
    let payload_json = serde_json::to_string(&payload)
        .context("failed to serialize discover printers command payload")?;
    enqueue_with_audit(
        database,
        tenant_id,
        agent_id,
        "discover_printers",
        &payload_json,
        audit_event(tenant_id, agent_id, actor, "agent.discover_printers"),
        "discover printers",
    )
    .await
}

pub async fn enqueue_diagnose_printer_with_audit(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
    payload: DiagnosePrinterPayload,
    actor: AuditActor,
) -> RepositoryResult<CommandRecord> {
    let payload_json = serde_json::to_string(&payload)
        .context("failed to serialize diagnose printer command payload")?;
    enqueue_with_audit(
        database,
        tenant_id,
        agent_id,
        "diagnose_printer",
        &payload_json,
        audit_event(tenant_id, agent_id, actor, "agent.diagnose_printer"),
        "diagnose printer",
    )
    .await
}

pub async fn enqueue_printer_operation_with_audit(
    database: &Database,
    tenant_id: TenantId,
    printer_id: &str,
    operation: PrinterOperationKind,
    actor: AuditActor,
) -> RepositoryResult<CommandRecord> {
    validate_printer_operation(&operation)?;
    let printer = ownership::printer_for_tenant(database, tenant_id, printer_id).await?;
    ownership::verify_agent_owner(database, tenant_id, printer.agent_id).await?;
    if !pandar_core::compatibility::live_controls_supported(printer.model.as_deref()) {
        return Err(RepositoryError::PrinterControlUnavailable);
    }

    let payload = PrinterOperationPayload {
        printer_id: printer.id.clone(),
        serial_number: printer.serial_number.clone(),
        operation,
    };
    let payload_json = serde_json::to_string(&payload)
        .context("failed to serialize printer operation command payload")?;
    let id = CommandId::new();
    let now = pandar_core::created_at_now();
    let connection = database.sea_orm_connection();
    let tx = connection
        .begin()
        .await
        .context("failed to begin printer operation command audit transaction")?;
    inserts::insert(
        &tx,
        InsertCommand {
            id,
            tenant_id,
            agent_id: printer.agent_id,
            printer_id: Some(&printer.id),
            kind: "printer_operation",
            payload_json: &payload_json,
            created_at: &now,
        },
    )
    .await?;
    insert_audit_event_tx(
        &tx,
        &printer_operation_audit_event(tenant_id, &printer, &payload.operation, actor),
    )
    .await?;
    tx.commit()
        .await
        .context("failed to commit printer operation command audit transaction")?;

    get_command(database, id)
        .await?
        .ok_or(RepositoryError::MissingCommand)
}

async fn enqueue_with_audit(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
    kind: &'static str,
    payload_json: &str,
    event: crate::repositories::AuditEvent,
    context_label: &'static str,
) -> RepositoryResult<CommandRecord> {
    ownership::verify_agent_owner(database, tenant_id, agent_id).await?;
    let id = CommandId::new();
    let now = pandar_core::created_at_now();
    let connection = database.sea_orm_connection();
    let tx = connection
        .begin()
        .await
        .with_context(|| format!("failed to begin {context_label} command audit transaction"))?;
    inserts::insert(
        &tx,
        insert_command(id, tenant_id, agent_id, kind, payload_json, &now),
    )
    .await?;
    insert_audit_event_tx(&tx, &event).await?;
    tx.commit()
        .await
        .with_context(|| format!("failed to commit {context_label} command audit transaction"))?;

    get_command(database, id)
        .await?
        .ok_or(RepositoryError::MissingCommand)
}

fn insert_command<'a>(
    id: CommandId,
    tenant_id: TenantId,
    agent_id: AgentId,
    kind: &'a str,
    payload_json: &'a str,
    now: &'a str,
) -> InsertCommand<'a> {
    InsertCommand {
        id,
        tenant_id,
        agent_id,
        printer_id: None,
        kind,
        payload_json,
        created_at: now,
    }
}

fn refresh_audit_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    actor: AuditActor,
) -> crate::repositories::AuditEvent {
    audit_event(tenant_id, agent_id, actor, "agent.refresh_printers")
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

fn audit_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    actor: AuditActor,
    action: &'static str,
) -> crate::repositories::AuditEvent {
    record_audit_event(
        tenant_id,
        actor,
        action,
        "agent",
        Some(agent_id.to_string()),
        serde_json::json!({}),
    )
}

async fn get_command(
    database: &Database,
    command_id: CommandId,
) -> RepositoryResult<Option<CommandRecord>> {
    crate::entities::commands::Entity::find_by_id(command_id.to_string())
        .one(&database.sea_orm_connection())
        .await
        .context("failed to get command")?
        .map(command_from_model)
        .transpose()
}
