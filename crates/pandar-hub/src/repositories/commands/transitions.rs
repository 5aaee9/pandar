use std::collections::HashSet;

use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandStatus, TenantId};
use sea_orm::{
    ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, EntityTrait, QueryFilter,
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    db::Database,
    entities::commands,
    repositories::{
        FirmwareControlPayload, FirmwarePersistedPhase, FirmwarePersistedResult,
        FirmwareRefreshPayload, PrinterOperationKind, PrinterOperationPayload, RepositoryError,
        RepositoryResult, agents::begin_stale_firmware_cleanup_transaction,
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
    update_status_if_current_on(&database.sea_orm_connection(), transition).await
}

pub(super) async fn update_status_if_current_on<C>(
    connection: &C,
    transition: StatusTransition<'_>,
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
        .exec(connection)
        .await
        .context("failed to update command status")?;

    Ok(result.rows_affected == 1)
}

pub async fn fail_stale_unowned_live_commands(
    database: &Database,
    now: &str,
    command_timeout: std::time::Duration,
    session_timeout: std::time::Duration,
    sweeper_instance_id: uuid::Uuid,
    owned_command_ids: &[CommandId],
) -> RepositoryResult<u64> {
    let command_timeout = time::Duration::try_from(command_timeout)
        .context("failed to convert live command timeout")?;
    let session_timeout = time::Duration::try_from(session_timeout)
        .context("failed to convert agent session timeout for live command cleanup")?;
    let now_at = OffsetDateTime::parse(now, &Rfc3339)
        .context("failed to parse live command cleanup timestamp")?;
    let command_cutoff = (now_at - command_timeout)
        .format(&Rfc3339)
        .context("failed to format live command cleanup cutoff")?;
    let session_cutoff_at = now_at - session_timeout;
    let owned_command_ids = owned_command_ids
        .iter()
        .map(ToString::to_string)
        .collect::<HashSet<_>>();
    let candidates = commands::Entity::find()
        .filter(
            Condition::any()
                .add(commands::Column::Kind.eq("link_printer"))
                .add(commands::Column::Kind.eq("printer_operation"))
                .add(commands::Column::Kind.eq("firmware_refresh"))
                .add(commands::Column::Kind.eq("firmware_control")),
        )
        .filter(
            Condition::any()
                .add(commands::Column::Status.eq(CommandStatus::Sent.as_str()))
                .add(commands::Column::Status.eq(CommandStatus::Acknowledged.as_str())),
        )
        .filter(commands::Column::UpdatedAt.lt(command_cutoff))
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
        let (error, firmware_owner) = match command.kind.as_str() {
            "link_printer" => (
                Some("printer link dispatch expired before completion"),
                None,
            ),
            "printer_operation" => {
                let payload: PrinterOperationPayload = serde_json::from_str(&command.payload_json)
                    .context("failed to deserialize stale printer operation command payload")?;
                (
                    matches!(
                        payload.operation,
                        PrinterOperationKind::HandlePrintError { .. }
                    )
                    .then_some("live printer operation owner unavailable before completion"),
                    None,
                )
            }
            "firmware_refresh" => {
                let payload: FirmwareRefreshPayload =
                    serde_json::from_str(&command.payload_json)
                        .context("failed to deserialize stale firmware refresh command payload")?;
                (
                    Some("live firmware refresh owner unavailable before completion"),
                    Some((payload.owner_session_id, payload.owner_instance_id)),
                )
            }
            "firmware_control" => {
                let payload: FirmwareControlPayload =
                    serde_json::from_str(&command.payload_json)
                        .context("failed to deserialize stale firmware control command payload")?;
                (
                    Some("live firmware control owner unavailable before completion"),
                    Some((payload.owner_session_id, payload.owner_instance_id)),
                )
            }
            _ => unreachable!("candidate query limits live command kinds"),
        };
        let Some(error) = error else {
            continue;
        };
        let result_json = matches!(
            command.kind.as_str(),
            "firmware_refresh" | "firmware_control"
        )
        .then(|| {
            serde_json::to_string(&FirmwarePersistedResult {
                phase: if command.status == CommandStatus::Acknowledged {
                    FirmwarePersistedPhase::OutcomeUnknown
                } else {
                    FirmwarePersistedPhase::PrePublishFailure
                },
                outcome: None,
                transient_status: None,
            })
            .context("failed to serialize stale firmware cleanup result")
        })
        .transpose()?;
        let transition = StatusTransition {
            command_id: command.id,
            tenant_id: command.tenant_id,
            agent_id: command.agent_id,
            status: CommandStatus::Failed,
            error: Some(error.to_owned()),
            result_json,
            allowed_statuses: &[CommandStatus::Sent, CommandStatus::Acknowledged],
        };
        let updated = if let Some((owner_session_id, owner_instance_id)) = firmware_owner {
            let Some(transaction) = begin_stale_firmware_cleanup_transaction(
                database,
                command.tenant_id,
                command.agent_id,
                &owner_session_id,
                owner_instance_id,
                sweeper_instance_id,
                session_cutoff_at,
            )
            .await?
            else {
                continue;
            };
            let updated = update_status_if_current_on(&transaction, transition).await?;
            transaction
                .commit()
                .await
                .context("failed to commit stale firmware command cleanup")?;
            updated
        } else {
            update_status_if_current(database, transition).await?
        };
        if updated {
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
