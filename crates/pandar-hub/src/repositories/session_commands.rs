use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandRecord, CommandStatus, JobStatus, TenantId};
use sea_orm::{ConnectionTrait, EntityTrait};

use crate::{
    db::Database,
    entities::commands,
    repositories::{
        RepositoryError, RepositoryResult,
        agents::{begin_closing_agent_transaction, begin_current_agent_transaction},
        commands::{rows::command_from_model, transitions},
        jobs,
    },
};

#[derive(Debug)]
pub(crate) enum CurrentSessionCommandAction {
    Send,
    Acknowledge,
    FailQueued {
        error: String,
    },
    Succeed {
        result_json: Option<String>,
    },
    Fail {
        error: String,
        result_json: Option<String>,
    },
}

pub(crate) async fn transition_current_session_command(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
    session_id: &str,
    command_id: CommandId,
    action: CurrentSessionCommandAction,
) -> RepositoryResult<CommandRecord> {
    let transaction =
        begin_current_agent_transaction(database, tenant_id, agent_id, session_id).await?;
    let command =
        transition_command_on(&transaction, tenant_id, agent_id, command_id, action).await?;
    transaction
        .commit()
        .await
        .context("failed to commit current-session command transition")?;
    Ok(command)
}

pub(crate) async fn fail_queued_for_closing_session(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
    closing_session_id: &str,
    command_id: CommandId,
    error: String,
) -> RepositoryResult<CommandRecord> {
    let transaction =
        begin_closing_agent_transaction(database, tenant_id, agent_id, closing_session_id).await?;
    let command = transition_command_on(
        &transaction,
        tenant_id,
        agent_id,
        command_id,
        CurrentSessionCommandAction::FailQueued { error },
    )
    .await?;
    transaction
        .commit()
        .await
        .context("failed to commit closing-session command transition")?;
    Ok(command)
}

async fn transition_command_on<C>(
    connection: &C,
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
    action: CurrentSessionCommandAction,
) -> RepositoryResult<CommandRecord>
where
    C: ConnectionTrait,
{
    let command = load_owned_command(connection, tenant_id, agent_id, command_id).await?;
    if command.kind == "print_project_file" {
        transition_print_command(connection, tenant_id, agent_id, command_id, action).await
    } else {
        transition_command(connection, command, action).await
    }
}

async fn transition_print_command<C>(
    connection: &C,
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
    action: CurrentSessionCommandAction,
) -> RepositoryResult<CommandRecord>
where
    C: ConnectionTrait,
{
    let (command_status, job_status, error, result_json, allowed_statuses, action) = match action {
        CurrentSessionCommandAction::Send => (
            CommandStatus::Sent,
            JobStatus::Sent,
            None,
            None,
            &[CommandStatus::Queued][..],
            "send",
        ),
        CurrentSessionCommandAction::Acknowledge => (
            CommandStatus::Acknowledged,
            JobStatus::Acknowledged,
            None,
            None,
            &[CommandStatus::Sent][..],
            "acknowledge",
        ),
        CurrentSessionCommandAction::FailQueued { error } => (
            CommandStatus::Failed,
            JobStatus::Failed,
            Some(error),
            None,
            &[CommandStatus::Queued][..],
            "fail",
        ),
        CurrentSessionCommandAction::Succeed { result_json } => (
            CommandStatus::Succeeded,
            JobStatus::Succeeded,
            None,
            result_json,
            &[CommandStatus::Sent, CommandStatus::Acknowledged][..],
            "succeed",
        ),
        CurrentSessionCommandAction::Fail { error, result_json } => (
            CommandStatus::Failed,
            JobStatus::Failed,
            Some(error),
            result_json,
            &[CommandStatus::Sent, CommandStatus::Acknowledged][..],
            "fail",
        ),
    };
    jobs::transitions::transition_print_command(
        connection,
        jobs::transitions::PrintCommandTransition {
            command_id,
            tenant_id,
            agent_id,
            command_status,
            job_status,
            error,
            result_json,
            allowed_statuses,
            action,
        },
    )
    .await
}

async fn transition_command<C>(
    connection: &C,
    command: CommandRecord,
    action: CurrentSessionCommandAction,
) -> RepositoryResult<CommandRecord>
where
    C: ConnectionTrait,
{
    let (status, error, result_json, allowed_statuses, action, terminal) = match action {
        CurrentSessionCommandAction::Send => (
            CommandStatus::Sent,
            None,
            None,
            &[CommandStatus::Queued][..],
            "send",
            false,
        ),
        CurrentSessionCommandAction::Acknowledge => (
            CommandStatus::Acknowledged,
            None,
            None,
            &[CommandStatus::Sent][..],
            "acknowledge",
            false,
        ),
        CurrentSessionCommandAction::FailQueued { error } => (
            CommandStatus::Failed,
            Some(error),
            None,
            &[CommandStatus::Queued][..],
            "fail",
            false,
        ),
        CurrentSessionCommandAction::Succeed { result_json } => (
            CommandStatus::Succeeded,
            None,
            result_json,
            &[CommandStatus::Sent, CommandStatus::Acknowledged][..],
            "succeed",
            true,
        ),
        CurrentSessionCommandAction::Fail { error, result_json } => (
            CommandStatus::Failed,
            Some(error),
            result_json,
            &[CommandStatus::Sent, CommandStatus::Acknowledged][..],
            "fail",
            true,
        ),
    };
    if terminal
        && matches!(
            command.kind.as_str(),
            "firmware_refresh" | "firmware_control"
        )
    {
        return Err(RepositoryError::InvalidCommandTransition {
            from: command.status.as_str().to_owned(),
            action: "finish firmware command through generic transition",
        });
    }
    let updated = transitions::update_status_if_current_on(
        connection,
        transitions::StatusTransition {
            command_id: command.id,
            tenant_id: command.tenant_id,
            agent_id: command.agent_id,
            status: status.clone(),
            error,
            result_json,
            allowed_statuses,
        },
    )
    .await?;
    let command =
        load_owned_command(connection, command.tenant_id, command.agent_id, command.id).await?;
    if updated || (terminal && command.status == status) {
        return Ok(command);
    }
    Err(transitions::invalid_transition(command.status, action))
}

async fn load_owned_command<C>(
    connection: &C,
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
) -> RepositoryResult<CommandRecord>
where
    C: ConnectionTrait,
{
    let command = commands::Entity::find_by_id(command_id.to_string())
        .one(connection)
        .await
        .context("failed to load command in current-session transaction")?
        .map(command_from_model)
        .transpose()?
        .ok_or(RepositoryError::MissingCommand)?;
    if command.tenant_id != tenant_id || command.agent_id != agent_id {
        return Err(RepositoryError::CommandOwnershipMismatch);
    }
    Ok(command)
}
