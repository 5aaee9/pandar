use pandar_core::{AgentId, TenantId};
use tokio::{sync::mpsc, task::JoinSet};
use tokio_stream::{Stream, StreamExt};
use tonic::Status;

use crate::{
    AppState,
    grpc::commands::repository_status,
    grpc::print_reports::handle_print_report,
    grpc::printer_materials::handle_materials_snapshot,
    grpc::printer_snapshots::{apply_snapshot, handle_snapshot, parse_snapshot},
    repositories::RepositoryError,
    sessions::{AgentSession, SessionToken, live_commands::fail_pending_live_commands},
};
use pandar_protocol::agent::v1::{AgentEvent, agent_event};

use super::validate_rfc3339;

mod commands;
#[path = "printer_device_features.rs"]
mod printer_device_features;

#[cfg(test)]
pub(super) use commands::{handle_ack, handle_result};
use commands::{handle_command_ack, handle_command_result};

/// Printer snapshots apply off the serial inbound loop so a slow snapshot
/// cannot starve the events behind it; the bound keeps a saturated handler
/// applying backpressure to the agent stream instead of spawning unboundedly.
const MAX_CONCURRENT_SNAPSHOT_APPLICATIONS: usize = 8;

pub(super) fn spawn_inbound_handler(
    state: AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    mut inbound: impl Stream<Item = Result<AgentEvent, Status>> + Send + Unpin + 'static,
    status_sender: mpsc::Sender<Status>,
) {
    tokio::spawn(async move {
        let mut snapshot_tasks = JoinSet::new();
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

            if let Err(err) = validate_event_identity(&event, tenant_id, agent_id) {
                tracing::error!(error = ?err, "failed to handle agent event");
                let _ = status_sender.send(err).await;
                break;
            }

            let AgentEvent {
                tenant_id: event_tenant_id,
                agent_id: event_agent_id,
                event_id,
                event: kind,
            } = event;
            let Some(agent_event::Event::PrinterSnapshot(snapshot)) = kind else {
                let event = AgentEvent {
                    tenant_id: event_tenant_id,
                    agent_id: event_agent_id,
                    event_id,
                    event: kind,
                };
                if let Err(err) = handle_event(&state, tenant_id, agent_id, token, event).await {
                    tracing::error!(error = ?err, "failed to handle agent event");
                    let _ = status_sender.send(err).await;
                    break;
                }
                continue;
            };

            // Snapshot payload validation stays on the serial loop so a
            // malformed snapshot keeps failing the stream fast; the database
            // transaction and event fanout apply concurrently.
            let parsed = match parse_snapshot(snapshot) {
                Ok(parsed) => parsed,
                Err(err) => {
                    tracing::error!(error = ?err, "failed to handle agent event");
                    let _ = status_sender.send(err).await;
                    break;
                }
            };
            while snapshot_tasks.len() >= MAX_CONCURRENT_SNAPSHOT_APPLICATIONS {
                if snapshot_tasks.join_next().await.is_none() {
                    break;
                }
            }
            let task_state = state.clone();
            snapshot_tasks.spawn(async move {
                if let Err(err) =
                    apply_snapshot(&task_state, tenant_id, agent_id, token, parsed).await
                {
                    tracing::error!(
                        error = ?err,
                        "failed to apply agent printer snapshot"
                    );
                }
            });
        }

        while let Some(result) = snapshot_tasks.join_next().await {
            if let Err(err) = result {
                tracing::error!(error = ?err, "agent printer snapshot task failed to join");
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
            state
                .publish_agent_printers_projection_changes(tenant_id, agent_id)
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

fn validate_event_identity(
    event: &AgentEvent,
    tenant_id: TenantId,
    agent_id: AgentId,
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
    Ok(())
}

pub(super) async fn handle_event(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    event: AgentEvent,
) -> Result<(), Status> {
    validate_event_identity(&event, tenant_id, agent_id)?;

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
