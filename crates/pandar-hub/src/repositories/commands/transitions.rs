use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandStatus, TenantId};
use sea_orm::{ActiveValue::Set, ColumnTrait, Condition, EntityTrait, QueryFilter};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    db::Database,
    entities::commands,
    repositories::{RepositoryError, RepositoryResult},
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

pub async fn fail_stale_unowned_link_printer_commands(
    database: &Database,
    now: &str,
    timeout: std::time::Duration,
    owned_command_ids: &[CommandId],
) -> RepositoryResult<u64> {
    let timeout = time::Duration::try_from(timeout)
        .context("failed to convert link printer command timeout")?;
    let cutoff = (OffsetDateTime::parse(now, &Rfc3339)
        .context("failed to parse link printer command cleanup timestamp")?
        - timeout)
        .format(&Rfc3339)
        .context("failed to format link printer command cleanup cutoff")?;

    let mut update = commands::Entity::update_many()
        .set(commands::ActiveModel {
            status: Set(CommandStatus::Failed.as_str().to_owned()),
            error: Set(Some(
                "printer link dispatch expired before completion".to_owned(),
            )),
            updated_at: Set(now.to_owned()),
            ..Default::default()
        })
        .filter(commands::Column::Kind.eq("link_printer"))
        .filter(
            Condition::any()
                .add(commands::Column::Status.eq(CommandStatus::Sent.as_str()))
                .add(commands::Column::Status.eq(CommandStatus::Acknowledged.as_str())),
        )
        .filter(commands::Column::UpdatedAt.lt(cutoff));

    if !owned_command_ids.is_empty() {
        update = update.filter(
            commands::Column::Id.is_not_in(
                owned_command_ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ),
        );
    }

    let result = update
        .exec(&database.sea_orm_connection())
        .await
        .context("failed to fail stale link printer commands")?;

    Ok(result.rows_affected)
}

pub(super) fn invalid_transition(status: CommandStatus, action: &'static str) -> RepositoryError {
    RepositoryError::InvalidCommandTransition {
        from: status.as_str().to_string(),
        action,
    }
}
