use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandRecord, TenantId};

use crate::{
    db::Database,
    repositories::{
        RecordAuditEvent, RepositoryError, RepositoryResult,
        audit::{build_audit_event, insert_audit_event_postgres, insert_audit_event_sqlite},
        commands::{
            inserts::{self, InsertCommand},
            ownership,
            rows::command_from_row,
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
    match database {
        Database::Sqlite(pool) => {
            let mut transaction = pool
                .begin()
                .await
                .context("failed to begin SQLite refresh command audit transaction")?;
            inserts::insert_sqlite(
                &mut *transaction,
                insert_command(id, tenant_id, agent_id, &now),
            )
            .await?;
            let event = refresh_audit_event(tenant_id, agent_id, user_id);
            insert_audit_event_sqlite(&mut *transaction, &event).await?;
            transaction
                .commit()
                .await
                .context("failed to commit SQLite refresh command audit transaction")?;
        }
        Database::Postgres(pool) => {
            let mut transaction = pool
                .begin()
                .await
                .context("failed to begin PostgreSQL refresh command audit transaction")?;
            inserts::insert_postgres(
                &mut *transaction,
                insert_command(id, tenant_id, agent_id, &now),
            )
            .await?;
            let event = refresh_audit_event(tenant_id, agent_id, user_id);
            insert_audit_event_postgres(&mut *transaction, &event).await?;
            transaction
                .commit()
                .await
                .context("failed to commit PostgreSQL refresh command audit transaction")?;
        }
    }

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
    match database {
        Database::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT id, tenant_id, agent_id, printer_id, kind, status, payload_json, error, created_at, updated_at
                 FROM commands
                 WHERE id = ?1",
            )
            .bind(command_id.to_string())
            .fetch_optional(pool)
            .await
            .context("failed to get SQLite command")?;
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
            .context("failed to get PostgreSQL command")?;
            row.map(command_from_row).transpose()
        }
    }
}
