use pandar_core::{AgentId, CommandId, CommandStatus, TenantId};
use tonic::{Code, Status};

use crate::{
    AppState,
    grpc::commands::{
        handle_ack_and_job, handle_result_and_job, parse_command_id, repository_status,
    },
    printer_events::{PrinterEvent, PrinterEventCommand},
    protocol::agent::v1::{CommandAck, CommandResult},
    repositories::{PrinterOperationKind, PrinterOperationPayload},
    sessions::{LiveCommandClaimOutcome, SessionToken},
};

pub(super) async fn handle_command_ack(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    ack: CommandAck,
) -> Result<(), Status> {
    let command_id = parse_command_id(&ack.command_id)?;
    let accepted = ack.accepted;
    let error = ack.error;
    match state
        .sessions()
        .claim_current_live_command(tenant_id, agent_id, token, command_id)
        .await
    {
        LiveCommandClaimOutcome::Claim(claim) => {
            let result = handle_ack_and_job(
                state,
                tenant_id,
                agent_id,
                command_id,
                accepted,
                error,
                claim.access_code(),
            )
            .await;
            if result.is_ok() && !accepted {
                claim.remove_pending();
            }
            result
        }
        LiveCommandClaimOutcome::NotCurrent => Ok(()),
        LiveCommandClaimOutcome::NotPending => {
            if !durable_fallback_allowed(state, tenant_id, agent_id, command_id).await? {
                return Ok(());
            }
            match state
                .sessions()
                .while_current(agent_id, token, || {
                    handle_ack_and_job(
                        state, tenant_id, agent_id, command_id, accepted, error, None,
                    )
                })
                .await
            {
                Some(result) => result,
                None => Ok(()),
            }
        }
    }
}

pub(super) async fn handle_command_result(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    result: CommandResult,
) -> Result<(), Status> {
    let command_id = parse_command_id(&result.command_id)?;
    let result_error = result.error.clone();
    let result_json = result.result_json.clone();
    match state
        .sessions()
        .claim_current_live_command(tenant_id, agent_id, token, command_id)
        .await
    {
        LiveCommandClaimOutcome::Claim(claim) => {
            if let Some(access_code) = claim.access_code()
                && link_printer_command_is_terminal(state, tenant_id, agent_id, command_id).await?
            {
                log_late_link_printer_result(
                    command_id,
                    &Status::failed_precondition("link printer command is already terminal"),
                    &result_error,
                    &result_json,
                    access_code,
                );
                return Ok(());
            }

            match handle_result_for_command(
                state,
                tenant_id,
                agent_id,
                command_id,
                result,
                claim.access_code(),
            )
            .await
            {
                Ok(()) => {
                    claim.remove_pending();
                    Ok(())
                }
                Err(err)
                    if err.code() == Code::FailedPrecondition && claim.access_code().is_some() =>
                {
                    log_late_link_printer_result(
                        command_id,
                        &err,
                        &result_error,
                        &result_json,
                        claim.access_code().unwrap_or_default(),
                    );
                    Ok(())
                }
                Err(err) => Err(err),
            }
        }
        LiveCommandClaimOutcome::NotCurrent => Ok(()),
        LiveCommandClaimOutcome::NotPending => {
            if !durable_fallback_allowed(state, tenant_id, agent_id, command_id).await? {
                return Ok(());
            }
            match state
                .sessions()
                .while_current(agent_id, token, || {
                    handle_result_for_command(state, tenant_id, agent_id, command_id, result, None)
                })
                .await
            {
                Some(result) => result,
                None => Ok(()),
            }
        }
    }
}

async fn durable_fallback_allowed(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
) -> Result<bool, Status> {
    let Some(command) = state
        .commands()
        .get_for_tenant(tenant_id, command_id)
        .await
        .map_err(repository_status)?
    else {
        return Err(Status::not_found("command not found"));
    };
    if command.agent_id != agent_id {
        return Err(Status::permission_denied(
            "command does not belong to authenticated agent",
        ));
    }

    match command.kind.as_str() {
        "link_printer" => Ok(false),
        "printer_operation" => {
            let payload: PrinterOperationPayload = serde_json::from_str(&command.payload_json)
                .map_err(|err| {
                    let err = anyhow::Error::new(err)
                        .context("failed to parse persisted printer operation command payload");
                    tracing::error!(
                        %command_id,
                        error = %format!("{err:#}"),
                        "failed to classify command for durable acknowledgement/result fallback"
                    );
                    Status::internal("invalid printer operation command payload")
                })?;
            Ok(!matches!(
                payload.operation,
                PrinterOperationKind::HandlePrintError { .. }
            ))
        }
        _ => Ok(true),
    }
}

#[cfg(test)]
pub(in crate::grpc) async fn handle_ack(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    ack: CommandAck,
) -> Result<(), Status> {
    let command_id = parse_command_id(&ack.command_id)?;
    handle_ack_and_job(
        state,
        tenant_id,
        agent_id,
        command_id,
        ack.accepted,
        ack.error,
        None,
    )
    .await
}

#[cfg(test)]
pub(in crate::grpc) async fn handle_result(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    result: CommandResult,
) -> Result<CommandId, Status> {
    let command_id = parse_command_id(&result.command_id)?;
    handle_result_for_command(state, tenant_id, agent_id, command_id, result, None).await?;
    Ok(command_id)
}

async fn handle_result_for_command(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
    result: CommandResult,
    link_printer_access_code: Option<&str>,
) -> Result<(), Status> {
    let command = handle_result_and_job(
        state,
        tenant_id,
        agent_id,
        command_id,
        result,
        link_printer_access_code,
    )
    .await?;
    if let Some(command) = command {
        state
            .printer_events()
            .publish_local(
                tenant_id,
                PrinterEvent::CommandResult {
                    command: Box::new(PrinterEventCommand::from(command)),
                },
            )
            .await;
    }
    Ok(())
}

async fn link_printer_command_is_terminal(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
) -> Result<bool, Status> {
    let Some(command) = state
        .commands()
        .get_for_tenant(tenant_id, command_id)
        .await
        .map_err(repository_status)?
    else {
        return Err(Status::not_found("command not found"));
    };
    if command.agent_id != agent_id {
        return Err(Status::permission_denied(
            "command does not belong to authenticated agent",
        ));
    }

    Ok(command.kind == "link_printer"
        && matches!(
            command.status,
            CommandStatus::Succeeded | CommandStatus::Failed
        ))
}

fn log_late_link_printer_result(
    command_id: CommandId,
    err: &Status,
    result_error: &str,
    result_json: &str,
    access_code: &str,
) {
    let error = crate::redaction::redact_link_printer_secret(&format!("{err:#}"), access_code);
    let result_error = crate::redaction::redact_link_printer_secret(result_error, access_code);
    let result_json = crate::redaction::redact_link_printer_result_json(result_json, access_code);
    tracing::warn!(
        command_id = %command_id,
        error = %error,
        command_result_error = %result_error,
        command_result_json = %result_json,
        "ignored late live printer link command result after terminal transition"
    );
}
