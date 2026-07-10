use std::collections::HashSet;

use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandStatus, TenantId};
use sea_orm::{ActiveValue::Set, ColumnTrait, Condition, EntityTrait, QueryFilter};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    db::Database,
    entities::commands,
    repositories::{
        PrinterOperationKind, PrinterOperationPayload, RepositoryError, RepositoryResult,
        commands::rows::command_from_model,
    },
};

pub struct StatusTransition<'a> {
    pub command_id: CommandId,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub status: CommandStatus,
    pub error: Option<String>,
    pub result_json: Option<String>,
    pub allowed_statuses: &'a [CommandStatus],
}

pub(super) struct CommandTransition<'a> {
    pub(super) command_id: CommandId,
    pub(super) tenant_id: TenantId,
    pub(super) agent_id: AgentId,
    pub(super) next_status: CommandStatus,
    pub(super) error: Option<String>,
    pub(super) allowed_statuses: &'a [CommandStatus],
    pub(super) action: &'static str,
}

pub(super) struct TerminalCommandTransition {
    pub(super) command_id: CommandId,
    pub(super) tenant_id: TenantId,
    pub(super) agent_id: AgentId,
    pub(super) terminal_status: CommandStatus,
    pub(super) error: Option<String>,
    pub(super) result_json: Option<String>,
    pub(super) action: &'static str,
}

pub async fn update_status_if_current(
    database: &Database,
    transition: StatusTransition<'_>,
) -> RepositoryResult<bool> {
    let now = pandar_core::created_at_now();
    let allowed_status_values = transition
        .allowed_statuses
        .iter()
        .map(|status| status.as_str().to_owned())
        .collect::<Vec<_>>();

    let result = commands::Entity::update_many()
        .set(commands::ActiveModel {
            status: Set(transition.status.as_str().to_owned()),
            error: Set(transition.error),
            result_json: Set(transition.result_json),
            updated_at: Set(now),
            ..Default::default()
        })
        .filter(commands::Column::Id.eq(transition.command_id.to_string()))
        .filter(commands::Column::TenantId.eq(transition.tenant_id.to_string()))
        .filter(commands::Column::AgentId.eq(transition.agent_id.to_string()))
        .filter(commands::Column::Status.is_in(allowed_status_values))
        .exec(&database.sea_orm_connection())
        .await
        .context("failed to update command status")?;

    Ok(result.rows_affected == 1)
}

pub async fn fail_stale_unowned_live_commands(
    database: &Database,
    now: &str,
    timeout: std::time::Duration,
    owned_command_ids: &[CommandId],
) -> RepositoryResult<u64> {
    let timeout =
        time::Duration::try_from(timeout).context("failed to convert live command timeout")?;
    let cutoff = (OffsetDateTime::parse(now, &Rfc3339)
        .context("failed to parse live command cleanup timestamp")?
        - timeout)
        .format(&Rfc3339)
        .context("failed to format live command cleanup cutoff")?;
    let owned_command_ids = owned_command_ids
        .iter()
        .map(ToString::to_string)
        .collect::<HashSet<_>>();
    let candidates = commands::Entity::find()
        .filter(
            Condition::any()
                .add(commands::Column::Kind.eq("link_printer"))
                .add(commands::Column::Kind.eq("printer_operation")),
        )
        .filter(
            Condition::any()
                .add(commands::Column::Status.eq(CommandStatus::Sent.as_str()))
                .add(commands::Column::Status.eq(CommandStatus::Acknowledged.as_str())),
        )
        .filter(commands::Column::UpdatedAt.lt(cutoff))
        .all(&database.sea_orm_connection())
        .await
        .context("failed to load stale live command candidates")?
        .into_iter()
        .map(command_from_model)
        .collect::<RepositoryResult<Vec<_>>>()?;

    let mut failed = 0;
    for command in candidates {
        if owned_command_ids.contains(&command.id.to_string()) {
            continue;
        }
        let error = match command.kind.as_str() {
            "link_printer" => Some("printer link dispatch expired before completion"),
            "printer_operation" => {
                let payload: PrinterOperationPayload = serde_json::from_str(&command.payload_json)
                    .context("failed to deserialize stale printer operation command payload")?;
                matches!(
                    payload.operation,
                    PrinterOperationKind::HandlePrintError { .. }
                )
                .then_some("live printer operation owner unavailable before completion")
            }
            _ => unreachable!("candidate query limits live command kinds"),
        };
        let Some(error) = error else {
            continue;
        };
        if update_status_if_current(
            database,
            StatusTransition {
                command_id: command.id,
                tenant_id: command.tenant_id,
                agent_id: command.agent_id,
                status: CommandStatus::Failed,
                error: Some(error.to_owned()),
                result_json: None,
                allowed_statuses: &[CommandStatus::Sent, CommandStatus::Acknowledged],
            },
        )
        .await?
        {
            failed += 1;
        }
    }

    Ok(failed)
}

pub(super) fn invalid_transition(status: CommandStatus, action: &'static str) -> RepositoryError {
    RepositoryError::InvalidCommandTransition {
        from: status.as_str().to_string(),
        action,
    }
}
