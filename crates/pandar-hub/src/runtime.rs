use std::time::Duration;

use anyhow::Context;
use futures_util::StreamExt;
use tokio::{sync::oneshot, task::JoinHandle};

use crate::{AppState, cluster::HubControlMessage, metrics::ControlPlaneMetric};

const STALE_SESSION_TIMEOUT: Duration = Duration::from_secs(45);
const STALE_SESSION_SWEEP_INTERVAL: Duration = Duration::from_secs(15);
const STALE_LINK_PRINTER_COMMAND_TIMEOUT: Duration = Duration::from_secs(300);

pub fn spawn_session_expiry(state: AppState) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(STALE_SESSION_SWEEP_INTERVAL);
        loop {
            ticker.tick().await;
            let now = pandar_core::created_at_now();
            if let Err(err) = expire_stale_sessions_once(&state, &now).await {
                tracing::error!(error = %format!("{err:#}"), "failed to expire stale agent sessions");
            }
            if let Err(err) = fail_stale_link_printer_commands_once(&state, &now).await {
                log_stale_link_printer_cleanup_error(&err);
            }
        }
    })
}

pub fn spawn_control_plane(state: AppState) -> JoinHandle<()> {
    spawn_control_plane_inner(state, None)
}

pub fn spawn_control_plane_ready(
    state: AppState,
) -> (JoinHandle<()>, oneshot::Receiver<anyhow::Result<()>>) {
    let (ready_sender, ready_receiver) = oneshot::channel();
    (
        spawn_control_plane_inner(state, Some(ready_sender)),
        ready_receiver,
    )
}

fn spawn_control_plane_inner(
    state: AppState,
    ready: Option<oneshot::Sender<anyhow::Result<()>>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut stream = match state.control_plane().subscribe().await {
            Ok(stream) => {
                if let Some(ready) = ready {
                    let _ = ready.send(Ok(()));
                }
                stream
            }
            Err(err) => {
                let err = err.context("failed to subscribe to hub control plane");
                if let Some(ready) = ready {
                    let _ = ready.send(Err(err));
                } else {
                    tracing::error!(error = %format!("{err:#}"), "failed to subscribe to hub control plane");
                }
                return;
            }
        };
        while let Some(message) = stream.next().await {
            match message {
                Ok(message) => {
                    handle_control_message(&state, message).await;
                    state
                        .metrics()
                        .record_control_plane(ControlPlaneMetric::ReceiveOk);
                }
                Err(err) => {
                    state
                        .metrics()
                        .record_control_plane(ControlPlaneMetric::ReceiveFailed);
                    tracing::error!(error = %format!("{err:#}"), "failed to receive hub control message");
                }
            }
        }
    })
}

async fn handle_control_message(state: &AppState, message: HubControlMessage) {
    match message {
        HubControlMessage::AgentWake {
            tenant_id,
            agent_id,
        } => match crate::cluster::parse_agent_identity(&tenant_id, &agent_id) {
            Ok((tenant_id, agent_id)) => {
                state.sessions().wake_local_agent(tenant_id, agent_id).await
            }
            Err(err) => {
                tracing::error!(error = %format!("{err:#}"), "failed to parse agent wake control message")
            }
        },
        HubControlMessage::AgentClose {
            tenant_id,
            agent_id,
        } => match crate::cluster::parse_agent_identity(&tenant_id, &agent_id) {
            Ok((tenant_id, agent_id)) => {
                state
                    .sessions()
                    .close_local_agent(tenant_id, agent_id)
                    .await
            }
            Err(err) => {
                tracing::error!(error = %format!("{err:#}"), "failed to parse agent close control message")
            }
        },
        HubControlMessage::PrinterEvent { tenant_id, event } => {
            match crate::cluster::parse_tenant_id(&tenant_id) {
                Ok(tenant_id) => state.printer_events().publish_local(tenant_id, event).await,
                Err(err) => {
                    tracing::error!(error = %format!("{err:#}"), "failed to parse printer event control message")
                }
            }
        }
    }
}

async fn expire_stale_sessions_once(state: &AppState, now: &str) -> anyhow::Result<usize> {
    expire_stale_sessions_with_timeout(state, now, STALE_SESSION_TIMEOUT).await
}

#[cfg_attr(not(test), allow(dead_code))]
async fn expire_stale_sessions_with_timeout(
    state: &AppState,
    now: &str,
    timeout: Duration,
) -> anyhow::Result<usize> {
    state
        .sessions()
        .expire_stale(now, timeout, state.agents())
        .await
        .context("failed to expire stale agent sessions")
        .map(|expired| expired.len())
}

#[cfg_attr(not(test), allow(dead_code))]
async fn fail_stale_link_printer_commands_once(state: &AppState, now: &str) -> anyhow::Result<u64> {
    fail_stale_link_printer_commands_with_timeout(state, now, STALE_LINK_PRINTER_COMMAND_TIMEOUT)
        .await
}

#[cfg_attr(not(test), allow(dead_code))]
async fn fail_stale_link_printer_commands_with_timeout(
    state: &AppState,
    now: &str,
    timeout: Duration,
) -> anyhow::Result<u64> {
    let pending = state.sessions().pending_live_command_ids().await;
    state
        .commands()
        .fail_stale_unowned_link_printer_commands(now, timeout, &pending)
        .await
        .context("failed to fail stale unowned link printer commands")
}

fn log_stale_link_printer_cleanup_error(err: &anyhow::Error) {
    tracing::error!(
        error = %crate::redaction::redact_secrets(&format!("{err:#}")),
        "failed to expire stale live printer link commands"
    );
}

#[cfg(test)]
mod tests;
