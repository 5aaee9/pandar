use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandRecord, TenantId};
use sea_orm::{EntityTrait, TransactionTrait};

use crate::{
    db::Database,
    repositories::{
        AuditActor, RepositoryError, RepositoryResult,
        audit::{insert_audit_event_tx, record_audit_event},
        commands::{
            DiagnosePrinterPayload, DiscoverPrintersPayload,
            inserts::{self, InsertCommand},
            ownership,
            rows::command_from_model,
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
