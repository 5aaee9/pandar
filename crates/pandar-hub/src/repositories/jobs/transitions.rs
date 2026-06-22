use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandRecord, CommandStatus, JobStatus, TenantId};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

use crate::{
    entities::{commands, jobs},
    repositories::{RepositoryError, RepositoryResult, commands::rows::command_from_model},
};

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

pub async fn transition_print_command<C>(
    connection: &C,
    transition: PrintCommandTransition<'_>,
) -> RepositoryResult<CommandRecord>
where
    C: ConnectionTrait,
{
    let updated = update_print_command(connection, &transition).await?;
    let command = load_owned_print_command(connection, &transition).await?;
    if updated || command.status == transition.command_status {
        update_job_for_command(connection, &transition).await?;
        return Ok(command);
    }

    Err(invalid_transition(command.status, transition.action))
}

async fn update_print_command<C>(
    connection: &C,
    transition: &PrintCommandTransition<'_>,
) -> RepositoryResult<bool>
where
    C: ConnectionTrait,
{
    let now = pandar_core::created_at_now();
    let allowed_status_values = transition
        .allowed_statuses
        .iter()
        .map(|status| status.as_str().to_owned())
        .collect::<Vec<_>>();

    let result = commands::Entity::update_many()
        .set(commands::ActiveModel {
            status: Set(transition.command_status.as_str().to_owned()),
            error: Set(transition.error.clone()),
            updated_at: Set(now),
            ..Default::default()
        })
        .filter(commands::Column::Id.eq(transition.command_id.to_string()))
        .filter(commands::Column::TenantId.eq(transition.tenant_id.to_string()))
        .filter(commands::Column::AgentId.eq(transition.agent_id.to_string()))
        .filter(commands::Column::Kind.eq("print_project_file"))
        .filter(commands::Column::Status.is_in(allowed_status_values))
        .exec(connection)
        .await
        .context("failed to update print command status")?;

    Ok(result.rows_affected == 1)
}

async fn update_job_for_command<C>(
    connection: &C,
    transition: &PrintCommandTransition<'_>,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    let result = jobs::Entity::update_many()
        .set(jobs::ActiveModel {
            status: Set(transition.job_status.as_str().to_owned()),
            error: Set(transition.error.clone()),
            updated_at: Set(pandar_core::created_at_now()),
            ..Default::default()
        })
        .filter(jobs::Column::CommandId.eq(transition.command_id.to_string()))
        .filter(jobs::Column::Status.is_not_in(["succeeded", "failed"]))
        .exec(connection)
        .await
        .context("failed to update print job for command")?;

    if result.rows_affected == 0
        && !matches!(
            transition.job_status,
            JobStatus::Succeeded | JobStatus::Failed
        )
    {
        return Err(RepositoryError::MissingJob);
    }
    Ok(())
}

async fn load_owned_print_command<C>(
    connection: &C,
    transition: &PrintCommandTransition<'_>,
) -> RepositoryResult<CommandRecord>
where
    C: ConnectionTrait,
{
    let command = commands::Entity::find_by_id(transition.command_id.to_string())
        .one(connection)
        .await
        .context("failed to load print command")?
        .map(command_from_model)
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
