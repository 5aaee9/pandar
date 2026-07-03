use pandar_core::{AgentId, AgentStatus, CommandId, CommandStatus, TenantId};
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt};
use tonic::{Code, Status};

use crate::{
    AppState,
    grpc::commands::{
        handle_ack_and_job, handle_result_and_job, parse_command_id, repository_status,
    },
    grpc::print_reports::handle_print_report,
    grpc::printer_materials::handle_materials_snapshot,
    grpc::printer_snapshots::handle_snapshot,
    printer_events::{PrinterEvent, PrinterEventCommand},
    protocol::agent::v1::{AgentEvent, CommandResult, agent_event},
    sessions::{AgentSession, SessionToken},
};

use super::validate_rfc3339;

pub(super) fn spawn_inbound_handler(
    state: AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    mut inbound: impl Stream<Item = Result<AgentEvent, Status>> + Send + Unpin + 'static,
    status_sender: mpsc::Sender<Status>,
) {
    tokio::spawn(async move {
        while let Some(event) = inbound.next().await {
            let event = match event {
                Ok(event) => event,
                Err(err) => {
                    tracing::error!(error = ?err, "failed to read agent event");
                    let _ = status_sender
                        .send(Status::internal("failed to read agent stream"))
                        .await;
                    break;
                }
            };

            if let Err(err) = handle_event(&state, tenant_id, agent_id, token, event).await {
                tracing::error!(error = ?err, "failed to handle agent event");
                let _ = status_sender.send(err).await;
                break;
            }
        }

        if let Some(session) = state.sessions().remove_if_current(agent_id, token).await {
            fail_pending_live_commands_on_close(&state, tenant_id, agent_id, session).await;
        }
    });
}

async fn fail_pending_live_commands_on_close(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    session: AgentSession,
) {
    let pending = session
        .pending_live_commands
        .lock()
        .expect("pending live commands mutex should not be poisoned")
        .drain()
        .map(|(command_id, _)| command_id)
        .collect::<Vec<_>>();
    for command_id in pending {
        if let Err(err) = state
            .commands()
            .mark_failed(
                command_id,
                tenant_id,
                agent_id,
                "agent connection closed before printer link completed",
            )
            .await
        {
            tracing::error!(
                command_id = %command_id,
                error = %crate::redaction::redact_secrets(&format!("{err:#}")),
                "failed to fail pending live printer link command after agent stream closed"
            );
        }
    }
}

pub(super) async fn handle_event(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    event: AgentEvent,
) -> Result<(), Status> {
    let event_tenant_id = TenantId::parse(&event.tenant_id)
        .map_err(|_| Status::invalid_argument("tenant_id must be a UUID"))?;
    let event_agent_id = AgentId::parse(&event.agent_id)
        .map_err(|_| Status::invalid_argument("agent_id must be a UUID"))?;
    if event_tenant_id != tenant_id || event_agent_id != agent_id {
        return Err(Status::permission_denied(
            "event identity does not match authenticated session",
        ));
    }

    match event.event {
        Some(agent_event::Event::Heartbeat(heartbeat)) => {
            validate_rfc3339(&heartbeat.observed_at)?;
            let Some(_) = state
                .sessions()
                .touch_heartbeat_if_current(agent_id, token, &heartbeat.observed_at)
                .await
            else {
                return Ok(());
            };
            state
                .agents()
                .update_connection(agent_id, AgentStatus::Online, None, &heartbeat.observed_at)
                .await
                .map_err(repository_status)?;
            Ok(())
        }
        Some(agent_event::Event::CommandAck(ack)) => {
            let command_id = parse_command_id(&ack.command_id)?;
            let pending_access_code = state
                .sessions()
                .pending_live_command_access_code(agent_id, token, command_id)
                .await;
            let accepted = ack.accepted;
            match state
                .sessions()
                .while_current(agent_id, token, || {
                    handle_ack_and_job(
                        state,
                        tenant_id,
                        agent_id,
                        command_id,
                        accepted,
                        ack.error,
                        pending_access_code.as_deref(),
                    )
                })
                .await
            {
                Some(Ok(())) => {
                    if !accepted {
                        state
                            .sessions()
                            .remove_pending_live_command(agent_id, token, command_id)
                            .await;
                    }
                    Ok(())
                }
                Some(Err(err)) => Err(err),
                None => Ok(()),
            }
        }
        Some(agent_event::Event::CommandResult(result)) => {
            let command_id = parse_command_id(&result.command_id)?;
            let pending_access_code = state
                .sessions()
                .pending_live_command_access_code(agent_id, token, command_id)
                .await;
            let result_error = result.error.clone();
            let result_json = result.result_json.clone();
            if let Some(access_code) = pending_access_code.as_deref()
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
            match state
                .sessions()
                .while_current(agent_id, token, || {
                    handle_result_for_command(
                        state,
                        tenant_id,
                        agent_id,
                        command_id,
                        result,
                        pending_access_code.as_deref(),
                    )
                })
                .await
            {
                Some(Ok(())) => {
                    state
                        .sessions()
                        .remove_pending_live_command(agent_id, token, command_id)
                        .await;
                    Ok(())
                }
                Some(Err(err))
                    if err.code() == Code::FailedPrecondition && pending_access_code.is_some() =>
                {
                    log_late_link_printer_result(
                        command_id,
                        &err,
                        &result_error,
                        &result_json,
                        pending_access_code.as_deref().unwrap_or_default(),
                    );
                    Ok(())
                }
                Some(Err(err)) => Err(err),
                None => Ok(()),
            }
        }
        Some(agent_event::Event::PrinterSnapshot(snapshot)) => {
            match state
                .sessions()
                .while_current(agent_id, token, || {
                    handle_snapshot(state, tenant_id, agent_id, snapshot)
                })
                .await
            {
                Some(result) => result,
                None => Ok(()),
            }
        }
        Some(agent_event::Event::PrintJobReport(report)) => {
            match state
                .sessions()
                .while_current(agent_id, token, || {
                    handle_print_report(state, tenant_id, agent_id, report)
                })
                .await
            {
                Some(result) => result,
                None => Ok(()),
            }
        }
        Some(agent_event::Event::PrinterMaterialsSnapshot(snapshot)) => {
            match state
                .sessions()
                .while_current(agent_id, token, || {
                    handle_materials_snapshot(state, tenant_id, agent_id, snapshot)
                })
                .await
            {
                Some(result) => result,
                None => Ok(()),
            }
        }
        Some(agent_event::Event::Hello(_)) | None => Ok(()),
    }
}

#[cfg(test)]
pub(super) async fn handle_ack(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    ack: crate::protocol::agent::v1::CommandAck,
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
pub(super) async fn handle_result(
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
