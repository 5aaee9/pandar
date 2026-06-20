use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandStatus, TenantId};

use crate::{db::Database, repositories::RepositoryResult};

pub async fn update_status_if_current(
    database: &Database,
    command_id: CommandId,
    tenant_id: TenantId,
    agent_id: AgentId,
    status: CommandStatus,
    error: Option<String>,
    allowed_statuses: &[CommandStatus],
) -> RepositoryResult<bool> {
    let now = pandar_core::created_at_now();
    let command_id = command_id.to_string();
    let tenant_id = tenant_id.to_string();
    let agent_id = agent_id.to_string();

    let rows_affected = match (database, allowed_statuses) {
        (Database::Sqlite(pool), [only]) => sqlx::query(
            "UPDATE commands
             SET status = ?4, error = ?5, updated_at = ?6
             WHERE id = ?1 AND tenant_id = ?2 AND agent_id = ?3 AND status = ?7",
        )
        .bind(&command_id)
        .bind(&tenant_id)
        .bind(&agent_id)
        .bind(status.as_str())
        .bind(error.as_deref())
        .bind(&now)
        .bind(only.as_str())
        .execute(pool)
        .await
        .context("failed to update SQLite command status")?
        .rows_affected(),
        (Database::Sqlite(pool), [first, second]) => sqlx::query(
            "UPDATE commands
             SET status = ?4, error = ?5, updated_at = ?6
             WHERE id = ?1 AND tenant_id = ?2 AND agent_id = ?3 AND status IN (?7, ?8)",
        )
        .bind(&command_id)
        .bind(&tenant_id)
        .bind(&agent_id)
        .bind(status.as_str())
        .bind(error.as_deref())
        .bind(&now)
        .bind(first.as_str())
        .bind(second.as_str())
        .execute(pool)
        .await
        .context("failed to update SQLite command status")?
        .rows_affected(),
        (Database::Postgres(pool), [only]) => sqlx::query(
            "UPDATE commands
             SET status = $4, error = $5, updated_at = $6
             WHERE id = $1 AND tenant_id = $2 AND agent_id = $3 AND status = $7",
        )
        .bind(&command_id)
        .bind(&tenant_id)
        .bind(&agent_id)
        .bind(status.as_str())
        .bind(error.as_deref())
        .bind(&now)
        .bind(only.as_str())
        .execute(pool)
        .await
        .context("failed to update PostgreSQL command status")?
        .rows_affected(),
        (Database::Postgres(pool), [first, second]) => sqlx::query(
            "UPDATE commands
             SET status = $4, error = $5, updated_at = $6
             WHERE id = $1 AND tenant_id = $2 AND agent_id = $3 AND status IN ($7, $8)",
        )
        .bind(&command_id)
        .bind(&tenant_id)
        .bind(&agent_id)
        .bind(status.as_str())
        .bind(error.as_deref())
        .bind(&now)
        .bind(first.as_str())
        .bind(second.as_str())
        .execute(pool)
        .await
        .context("failed to update PostgreSQL command status")?
        .rows_affected(),
        (_, _) => unreachable!("command status transitions have one or two allowed statuses"),
    };

    Ok(rows_affected == 1)
}
