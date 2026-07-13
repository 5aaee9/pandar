use pandar_core::{AgentId, TenantId};
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
    repositories::RepositoryError,
    sessions::{AgentSession, SessionToken, live_commands::fail_pending_live_commands},
};

use super::validate_rfc3339;

mod commands;
#[path = "printer_device_features.rs"]
mod printer_device_features;

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

        if let Some(session) = disconnect_session(&state, tenant_id, agent_id, token).await {
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

pub(super) async fn disconnect_session(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
) -> Option<AgentSession> {
    let _lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    if !state.sessions().is_current(agent_id, token).await {
        return None;
    }
    let now = pandar_core::created_at_now();
    if let Err(err) = state
        .agents()
        .mark_offline_if_current(tenant_id, agent_id, &token.persisted_id(), &now)
        .await
    {
        tracing::error!(
            error = %format!("{err:#}"),
            "failed to persist disconnected agent session"
        );
        return None;
    }
    state.sessions().remove_if_current(agent_id, token).await
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
            let _lease = state
                .sessions()
                .transition_lease_for_session(agent_id, token)
                .await;
            if !state.sessions().is_current(agent_id, token).await {
                return Ok(());
            }
            match state
                .agents()
                .heartbeat_if_current(
                    tenant_id,
                    agent_id,
                    &token.persisted_id(),
                    &heartbeat.observed_at,
                )
                .await
            {
                Ok(_) => {}
                Err(RepositoryError::AgentSessionNotCurrent) => return Ok(()),
                Err(err) => return Err(repository_status(err)),
            }
            state
                .sessions()
                .touch_heartbeat_if_current(agent_id, token, &heartbeat.observed_at)
                .await;
            Ok(())
        }
        Some(agent_event::Event::CommandAck(ack)) => {
            handle_command_ack(state, tenant_id, agent_id, token, ack).await
        }
        Some(agent_event::Event::CommandResult(result)) => {
            handle_command_result(state, tenant_id, agent_id, token, result).await
        }
        Some(agent_event::Event::PrinterSnapshot(snapshot)) => {
            handle_snapshot(state, tenant_id, agent_id, token, snapshot).await
        }
        Some(agent_event::Event::PrintJobReport(report)) => {
            handle_print_report(state, tenant_id, agent_id, token, report).await
        }
        Some(agent_event::Event::PrinterMaterialsSnapshot(snapshot)) => {
            handle_materials_snapshot(state, tenant_id, agent_id, token, snapshot).await
        }
        Some(agent_event::Event::PrinterDeviceFeaturesSnapshot(snapshot)) => {
            printer_device_features::handle_device_features_snapshot(
                state, tenant_id, agent_id, token, snapshot,
            )
            .await
        }
        Some(agent_event::Event::PrinterFirmwareModulesSnapshot(snapshot)) => {
            super::printer_firmware::handle_modules_snapshot(
                state, tenant_id, agent_id, token, snapshot,
            )
            .await
        }
        Some(agent_event::Event::PrinterFirmwareStatusSnapshot(snapshot)) => {
            super::printer_firmware::handle_status_snapshot(
                state, tenant_id, agent_id, token, snapshot,
            )
            .await
        }
        Some(agent_event::Event::PrinterFirmwareInvalidated(invalidated)) => {
            super::printer_firmware::handle_invalidated(
                state,
                tenant_id,
                agent_id,
                token,
                invalidated,
            )
            .await
        }
        Some(agent_event::Event::FirmwarePrepared(prepared)) => {
            super::printer_firmware::handle_prepared(state, tenant_id, agent_id, token, prepared)
                .await
        }
        Some(agent_event::Event::FirmwarePublished(published)) => {
            super::printer_firmware::handle_published(state, tenant_id, agent_id, token, published)
                .await
        }
        Some(agent_event::Event::Hello(_)) | None => Ok(()),
    }
}
