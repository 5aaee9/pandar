use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandRecord, CommandStatus, JobStatus, TenantId};

use crate::repositories::{RepositoryError, RepositoryResult, commands::rows::command_from_row};

pub struct PrintCommandTransition<'a> {
    pub command_id: CommandId,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub command_status: CommandStatus,
    pub job_status: JobStatus,
    pub error: Option<String>,
    pub allowed_statuses: &'a [CommandStatus],
    pub action: &'static str,
}

pub async fn transition_print_command_sqlite(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    transition: PrintCommandTransition<'_>,
) -> RepositoryResult<CommandRecord> {
    let updated = update_print_command_sqlite(&mut **transaction, &transition).await?;
    let command = load_owned_print_command_sqlite(&mut **transaction, &transition).await?;
    if updated || command.status == transition.command_status {
        update_job_for_command_sqlite(&mut **transaction, &transition).await?;
        return Ok(command);
    }

    Err(invalid_transition(command.status, transition.action))
}

pub async fn transition_print_command_postgres(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    transition: PrintCommandTransition<'_>,
) -> RepositoryResult<CommandRecord> {
    let updated = update_print_command_postgres(&mut **transaction, &transition).await?;
    let command = load_owned_print_command_postgres(&mut **transaction, &transition).await?;
    if updated || command.status == transition.command_status {
        update_job_for_command_postgres(&mut **transaction, &transition).await?;
        return Ok(command);
    }

    Err(invalid_transition(command.status, transition.action))
}

async fn update_print_command_sqlite(
    executor: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    transition: &PrintCommandTransition<'_>,
) -> RepositoryResult<bool> {
    let now = pandar_core::created_at_now();
    let rows_affected = match transition.allowed_statuses {
        [only] => sqlx::query(
            "UPDATE commands SET status = ?4, error = ?5, updated_at = ?6
             WHERE id = ?1 AND tenant_id = ?2 AND agent_id = ?3 AND kind = 'print_project_file' AND status = ?7",
        )
        .bind(transition.command_id.to_string())
        .bind(transition.tenant_id.to_string())
        .bind(transition.agent_id.to_string())
        .bind(transition.command_status.as_str())
        .bind(transition.error.as_deref())
        .bind(&now)
        .bind(only.as_str())
        .execute(executor)
        .await
        .context("failed to update SQLite print command status")?
        .rows_affected(),
        [first, second] => sqlx::query(
            "UPDATE commands SET status = ?4, error = ?5, updated_at = ?6
             WHERE id = ?1 AND tenant_id = ?2 AND agent_id = ?3 AND kind = 'print_project_file' AND status IN (?7, ?8)",
        )
        .bind(transition.command_id.to_string())
        .bind(transition.tenant_id.to_string())
        .bind(transition.agent_id.to_string())
        .bind(transition.command_status.as_str())
        .bind(transition.error.as_deref())
        .bind(&now)
        .bind(first.as_str())
        .bind(second.as_str())
        .execute(executor)
        .await
        .context("failed to update SQLite print command status")?
        .rows_affected(),
        _ => unreachable!("print command transitions have one or two allowed statuses"),
    };

    Ok(rows_affected == 1)
}

async fn update_print_command_postgres(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    transition: &PrintCommandTransition<'_>,
) -> RepositoryResult<bool> {
    let now = pandar_core::created_at_now();
    let rows_affected = match transition.allowed_statuses {
        [only] => sqlx::query(
            "UPDATE commands SET status = $4, error = $5, updated_at = $6
             WHERE id = $1 AND tenant_id = $2 AND agent_id = $3 AND kind = 'print_project_file' AND status = $7",
        )
        .bind(transition.command_id.to_string())
        .bind(transition.tenant_id.to_string())
        .bind(transition.agent_id.to_string())
        .bind(transition.command_status.as_str())
        .bind(transition.error.as_deref())
        .bind(&now)
        .bind(only.as_str())
        .execute(executor)
        .await
        .context("failed to update PostgreSQL print command status")?
        .rows_affected(),
        [first, second] => sqlx::query(
            "UPDATE commands SET status = $4, error = $5, updated_at = $6
             WHERE id = $1 AND tenant_id = $2 AND agent_id = $3 AND kind = 'print_project_file' AND status IN ($7, $8)",
        )
        .bind(transition.command_id.to_string())
        .bind(transition.tenant_id.to_string())
        .bind(transition.agent_id.to_string())
        .bind(transition.command_status.as_str())
        .bind(transition.error.as_deref())
        .bind(&now)
        .bind(first.as_str())
        .bind(second.as_str())
        .execute(executor)
        .await
        .context("failed to update PostgreSQL print command status")?
        .rows_affected(),
        _ => unreachable!("print command transitions have one or two allowed statuses"),
    };

    Ok(rows_affected == 1)
}

async fn update_job_for_command_sqlite(
    executor: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    transition: &PrintCommandTransition<'_>,
) -> RepositoryResult<()> {
    let rows_affected = sqlx::query(
        "UPDATE jobs SET status = ?2, error = ?3, updated_at = ?4
         WHERE command_id = ?1 AND status NOT IN ('succeeded', 'failed')",
    )
    .bind(transition.command_id.to_string())
    .bind(transition.job_status.as_str())
    .bind(transition.error.as_deref())
    .bind(pandar_core::created_at_now())
    .execute(executor)
    .await
    .context("failed to update SQLite print job for command")?
    .rows_affected();
    if rows_affected == 0
        && !matches!(
            transition.job_status,
            JobStatus::Succeeded | JobStatus::Failed
        )
    {
        return Err(RepositoryError::MissingJob);
    }
    Ok(())
}

async fn update_job_for_command_postgres(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    transition: &PrintCommandTransition<'_>,
) -> RepositoryResult<()> {
    let rows_affected = sqlx::query(
        "UPDATE jobs SET status = $2, error = $3, updated_at = $4
         WHERE command_id = $1 AND status NOT IN ('succeeded', 'failed')",
    )
    .bind(transition.command_id.to_string())
    .bind(transition.job_status.as_str())
    .bind(transition.error.as_deref())
    .bind(pandar_core::created_at_now())
    .execute(executor)
    .await
    .context("failed to update PostgreSQL print job for command")?
    .rows_affected();
    if rows_affected == 0
        && !matches!(
            transition.job_status,
            JobStatus::Succeeded | JobStatus::Failed
        )
    {
        return Err(RepositoryError::MissingJob);
    }
    Ok(())
}

async fn load_owned_print_command_sqlite(
    executor: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    transition: &PrintCommandTransition<'_>,
) -> RepositoryResult<CommandRecord> {
    let row = sqlx::query(
        "SELECT id, tenant_id, agent_id, printer_id, kind, status, payload_json, error, created_at, updated_at
         FROM commands WHERE id = ?1",
    )
    .bind(transition.command_id.to_string())
    .fetch_optional(executor)
    .await
    .context("failed to load SQLite print command")?;
    let command = row
        .map(command_from_row)
        .transpose()?
        .ok_or(RepositoryError::MissingCommand)?;
    verify_owned_print_command(command, transition)
}

async fn load_owned_print_command_postgres(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    transition: &PrintCommandTransition<'_>,
) -> RepositoryResult<CommandRecord> {
    let row = sqlx::query(
        "SELECT id, tenant_id, agent_id, printer_id, kind, status, payload_json, error, created_at, updated_at
         FROM commands WHERE id = $1",
    )
    .bind(transition.command_id.to_string())
    .fetch_optional(executor)
    .await
    .context("failed to load PostgreSQL print command")?;
    let command = row
        .map(command_from_row)
        .transpose()?
        .ok_or(RepositoryError::MissingCommand)?;
    verify_owned_print_command(command, transition)
}

fn verify_owned_print_command(
    command: CommandRecord,
    transition: &PrintCommandTransition<'_>,
) -> RepositoryResult<CommandRecord> {
    if command.tenant_id != transition.tenant_id || command.agent_id != transition.agent_id {
        return Err(RepositoryError::CommandOwnershipMismatch);
    }
    if command.kind != "print_project_file" {
        return Err(RepositoryError::MissingJob);
    }

    Ok(command)
}

fn invalid_transition(status: CommandStatus, action: &'static str) -> RepositoryError {
    RepositoryError::InvalidCommandTransition {
        from: status.as_str().to_string(),
        action,
    }
}
