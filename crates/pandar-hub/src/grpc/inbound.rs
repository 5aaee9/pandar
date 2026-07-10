use pandar_core::{AgentId, AgentStatus, TenantId};
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt};
use tonic::Status;

use crate::{
    AppState,
    grpc::commands::repository_status,
    grpc::print_reports::handle_print_report,
    grpc::printer_materials::handle_materials_snapshot,
    grpc::printer_snapshots::handle_snapshot,
    protocol::agent::v1::{AgentEvent, agent_event},
    sessions::{SessionToken, live_commands::fail_pending_live_commands},
};

use super::validate_rfc3339;

mod commands;

#[cfg(test)]
pub(super) use commands::{handle_ack, handle_result};
use commands::{handle_command_ack, handle_command_result};

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
            fail_pending_live_commands(
                &state,
                tenant_id,
                agent_id,
                session,
                "agent connection closed before printer operation completed",
            )
            .await;
        }
    });
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
            handle_command_ack(state, tenant_id, agent_id, token, ack).await
        }
        Some(agent_event::Event::CommandResult(result)) => {
            handle_command_result(state, tenant_id, agent_id, token, result).await
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
