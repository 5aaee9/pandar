use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandRecord, TenantId};
use sea_orm::{EntityTrait, TransactionTrait};

use crate::{
    db::Database,
    repositories::{
        RecordAuditEvent, RepositoryError, RepositoryResult,
        audit::{build_audit_event, insert_audit_event_tx},
        commands::{
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
    user_id: String,
) -> RepositoryResult<CommandRecord> {
    ownership::verify_agent_owner(database, tenant_id, agent_id).await?;
    let id = CommandId::new();
    let now = pandar_core::created_at_now();
    let connection = database.sea_orm_connection();
    let tx = connection
        .begin()
        .await
        .context("failed to begin refresh command audit transaction")?;
    inserts::insert(&tx, insert_command(id, tenant_id, agent_id, &now)).await?;
    let event = refresh_audit_event(tenant_id, agent_id, user_id);
    insert_audit_event_tx(&tx, &event).await?;
    tx.commit()
        .await
        .context("failed to commit refresh command audit transaction")?;

    get_command(database, id)
        .await?
        .ok_or(RepositoryError::MissingCommand)
}

fn insert_command<'a>(
    id: CommandId,
    tenant_id: TenantId,
    agent_id: AgentId,
    now: &'a str,
) -> InsertCommand<'a> {
    InsertCommand {
        id,
        tenant_id,
        agent_id,
        printer_id: None,
        kind: "refresh_printers",
        payload_json: "{}",
        created_at: now,
    }
}

fn refresh_audit_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    user_id: String,
) -> crate::repositories::AuditEvent {
    build_audit_event(RecordAuditEvent {
        tenant_id,
        actor_type: "user".to_owned(),
        user_id: Some(user_id),
        action: "agent.refresh_printers".to_owned(),
        target_type: "agent".to_owned(),
        target_id: Some(agent_id.to_string()),
        metadata_json: "{}".to_owned(),
    })
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
