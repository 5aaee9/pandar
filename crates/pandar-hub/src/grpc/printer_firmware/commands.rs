use pandar_core::{
    AgentId, CommandId, CommandStatus, FirmwareAcknowledgement as CoreAcknowledgement,
    FirmwareTerminalOutcome, PrinterFirmwareStatus as CoreFirmwareStatus, TenantId,
};
use tonic::Status;

use super::{
    core_module, core_upgrade_state, module_names_are_valid, required_serial, storage_value,
};
use crate::{
    AppState,
    firmware_control::{
        FirmwareExecutePhase, FirmwareExecuteResult, FirmwareRefreshResult, finish_agent_failure,
        finish_cancelled_commands, finish_pre_publish_failure,
    },
    grpc::commands::{parse_command_id, repository_status},
    protocol::agent::v1::{
        CommandResult, FirmwareCommandResult, FirmwarePrepared, FirmwarePublished,
        firmware_command_result,
    },
    repositories::{FirmwarePersistedPhase, FirmwarePersistedResult, PrinterFirmwareUpdateOutcome},
    sessions::{
        ClaimedFirmwareCommand, ClaimedFirmwareKind, FirmwareCommandIdentity, SessionToken,
    },
};

mod completion;
mod redaction;
mod support;
use completion::{complete_control, complete_refresh};
use redaction::redact_result_strings;
#[cfg(test)]
pub(crate) use support::completion_pause;
use support::{
    begin_current_session_fence, commit_current_session_fence, core_status, exact_identity,
    identity_matches_session,
};

pub(in crate::grpc) async fn handle_prepared(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    prepared: FirmwarePrepared,
) -> Result<(), Status> {
    let command_id = parse_command_id(&prepared.command_id)?;
    let serial = required_serial(&prepared.serial)?;
    let generation = storage_value(prepared.generation, "generation")?;
    let _lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    if !state.sessions().is_current(agent_id, token).await {
        return Ok(());
    }
    let Some(identity) = exact_identity(
        state, tenant_id, agent_id, token, command_id, &serial, generation,
    ) else {
        return Ok(());
    };
    let Some(fence) = begin_current_session_fence(state, tenant_id, agent_id, token).await? else {
        return Ok(());
    };
    state
        .sessions()
        .complete_firmware_prepared_under_transition(&identity);
    commit_current_session_fence(fence, "failed to release firmware prepared session fence")
        .await?;
    Ok(())
}

pub(in crate::grpc) async fn handle_published(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    published: FirmwarePublished,
) -> Result<(), Status> {
    let command_id = parse_command_id(&published.command_id)?;
    let serial = required_serial(&published.serial)?;
    let generation = storage_value(published.generation, "generation")?;
    let _lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    if !state.sessions().is_current(agent_id, token).await {
        return Ok(());
    }
    let Some(identity) = exact_identity(
        state, tenant_id, agent_id, token, command_id, &serial, generation,
    ) else {
        return Ok(());
    };
    let Some(fence) = begin_current_session_fence(state, tenant_id, agent_id, token).await? else {
        return Ok(());
    };
    state
        .sessions()
        .mark_firmware_published_under_transition(&identity);
    commit_current_session_fence(fence, "failed to release firmware published session fence")
        .await?;
    Ok(())
}

pub(in crate::grpc) async fn handle_command_ack(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    command_id: CommandId,
    accepted: bool,
    error: &str,
) -> Result<bool, Status> {
    let lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    if !state.sessions().is_current(agent_id, token).await {
        return Ok(false);
    }
    let Some(identity) = state.sessions().firmware_command_locator(command_id) else {
        return Ok(false);
    };
    if !identity_matches_session(&identity, tenant_id, agent_id, token) {
        return Ok(false);
    }
    let Some(fence) = begin_current_session_fence(state, tenant_id, agent_id, token).await? else {
        return Ok(false);
    };
    if accepted {
        commit_current_session_fence(fence, "failed to release firmware ack session fence").await?;
        return Ok(true);
    }
    let claimed = state
        .sessions()
        .claim_firmware_result_under_transition(&identity, Some(error))
        .expect("located firmware command must remain pending under transition lease");
    if let Err(status) =
        commit_current_session_fence(fence, "failed to release firmware rejection session fence")
            .await
    {
        drop(lease);
        finish_agent_failure(
            state,
            claimed,
            "firmware rejection session fence failed after claim",
        )
        .await;
        return Err(status);
    }
    drop(lease);
    finish_agent_failure(state, claimed, "agent rejected firmware command").await;
    Ok(true)
}

pub(in crate::grpc) async fn handle_command_failure(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    command_id: CommandId,
    error: &str,
) -> Result<bool, Status> {
    let lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    if !state.sessions().is_current(agent_id, token).await {
        return Ok(false);
    }
    let Some(identity) = state.sessions().firmware_command_locator(command_id) else {
        return Ok(false);
    };
    if !identity_matches_session(&identity, tenant_id, agent_id, token) {
        return Ok(false);
    }
    let Some(fence) = begin_current_session_fence(state, tenant_id, agent_id, token).await? else {
        return Ok(false);
    };
    let claimed = state
        .sessions()
        .claim_firmware_result_under_transition(&identity, Some(error))
        .expect("located firmware failure must remain pending under transition lease");
    if let Err(status) =
        commit_current_session_fence(fence, "failed to release firmware failure session fence")
            .await
    {
        drop(lease);
        finish_agent_failure(
            state,
            claimed,
            "firmware failure session fence failed after claim",
        )
        .await;
        return Err(status);
    }
    drop(lease);
    finish_agent_failure(state, claimed, "agent firmware command failed").await;
    Ok(true)
}

pub(in crate::grpc) async fn handle_command_result(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    outer: &CommandResult,
    mut result: FirmwareCommandResult,
) -> Result<(), Status> {
    let command_id = parse_command_id(&outer.command_id)?;
    if result.command_id != outer.command_id {
        return Err(Status::invalid_argument(
            "firmware result command id does not match outer command id",
        ));
    }
    let serial = required_serial(&result.serial)?;
    let generation = storage_value(result.generation, "generation")?;
    if result.outcome.is_none() {
        return Err(Status::invalid_argument(
            "firmware result outcome is required",
        ));
    }
    if let Some(firmware_command_result::Outcome::RefreshedModules(refreshed)) = &result.outcome {
        storage_value(refreshed.module_revision, "module_revision")?;
    }
    let lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    if !state.sessions().is_current(agent_id, token).await {
        return Ok(());
    }
    let Some(identity) = exact_identity(
        state, tenant_id, agent_id, token, command_id, &serial, generation,
    ) else {
        return Ok(());
    };
    let Some(fence) = begin_current_session_fence(state, tenant_id, agent_id, token).await? else {
        return Ok(());
    };
    redact_result_strings(state, &identity, &mut result);
    let outcome = result
        .outcome
        .take()
        .expect("firmware outcome presence was checked above");
    let claimed = state
        .sessions()
        .claim_firmware_typed_result_under_transition(&identity);
    let Some(claimed) = claimed else {
        return Ok(());
    };
    if let Err(status) =
        commit_current_session_fence(fence, "failed to release firmware result session fence").await
    {
        drop(lease);
        match claimed.kind {
            ClaimedFirmwareKind::Refresh => {
                finish_pre_publish_failure(
                    state,
                    claimed,
                    "firmware refresh session fence failed after result claim",
                )
                .await;
            }
            ClaimedFirmwareKind::Control => {
                finish_cancelled_commands(
                    state,
                    vec![claimed],
                    "firmware execute session fence failed after result claim",
                )
                .await;
            }
        }
        return Err(status);
    }
    drop(lease);
    #[cfg(test)]
    completion_pause::wait(command_id).await;
    match (claimed.kind, outcome) {
        (
            ClaimedFirmwareKind::Refresh,
            firmware_command_result::Outcome::RefreshedModules(value),
        ) => complete_refresh(state, claimed, value.module_revision, value.modules).await,
        (
            ClaimedFirmwareKind::Control,
            firmware_command_result::Outcome::Acknowledgement(value),
        ) => {
            let acknowledgement = CoreAcknowledgement {
                command: value.command,
                sequence_id: value.sequence_id,
                result: value.result,
                error_code: value.error_code,
                reason: value.reason,
                message: value.message,
            };
            let rejected = acknowledgement.error_code.is_some_and(|code| code != 0)
                || acknowledgement.result.as_deref() == Some("fail");
            complete_control(
                state,
                claimed,
                if rejected {
                    FirmwareExecutePhase::Rejected
                } else {
                    FirmwareExecutePhase::Acknowledged
                },
                FirmwareTerminalOutcome::Acknowledged { acknowledgement },
                result.transient_status.map(core_status),
            )
            .await
        }
        (
            ClaimedFirmwareKind::Control,
            firmware_command_result::Outcome::PublishedWithoutAcknowledgement(_),
        ) => {
            complete_control(
                state,
                claimed,
                FirmwareExecutePhase::OutcomeUnknown,
                FirmwareTerminalOutcome::PublishedWithoutAcknowledgement,
                result.transient_status.map(core_status),
            )
            .await
        }
        (kind, _) => {
            let label = match kind {
                ClaimedFirmwareKind::Refresh => "refresh",
                ClaimedFirmwareKind::Control => "control",
            };
            finish_cancelled_commands(
                state,
                vec![claimed],
                "agent returned a firmware outcome for the wrong command kind",
            )
            .await;
            Err(Status::invalid_argument(format!(
                "firmware outcome does not match pending {label} command"
            )))
        }
    }
}
