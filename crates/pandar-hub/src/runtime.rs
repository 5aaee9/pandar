use std::time::Duration;

use anyhow::Context;
use futures_util::StreamExt;
use tokio::{sync::oneshot, task::JoinHandle};

use crate::{
    AppState, cluster::HubControlMessage, metrics::ControlPlaneMetric,
    sessions::live_commands::fail_pending_live_commands,
};

const STALE_SESSION_TIMEOUT: Duration = Duration::from_secs(45);
const STALE_SESSION_SWEEP_INTERVAL: Duration = Duration::from_secs(15);
const STALE_LIVE_COMMAND_TIMEOUT: Duration = Duration::from_secs(300);

pub fn spawn_session_expiry(state: AppState) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(STALE_SESSION_SWEEP_INTERVAL);
        loop {
            ticker.tick().await;
            let now = pandar_core::created_at_now();
            if let Err(err) = expire_stale_sessions_once(&state, &now).await {
                tracing::error!(error = %format!("{err:#}"), "failed to expire stale agent sessions");
            }
            if let Err(err) = mark_stalled_pending_jobs_once(&state, &now).await {
                tracing::error!(error = %format!("{err:#}"), "failed to mark pending print jobs stalled");
            }
            if let Err(err) = fail_stale_live_commands_once(&state, &now).await {
                log_stale_live_command_cleanup_error(&err);
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
        let mut ready = ready;
        loop {
            let mut stream = match state.control_plane().subscribe().await {
                Ok(stream) => {
                    if let Some(ready) = ready.take() {
                        let _ = ready.send(Ok(()));
                    }
                    stream
                }
                Err(err) => {
                    let err = err.context("failed to subscribe to hub control plane");
                    state.printer_events().invalidate_epoch();
                    tracing::error!(error = %format!("{err:#}"), "failed to subscribe to hub control plane");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
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
                        state.printer_events().invalidate_epoch();
                        tracing::error!(error = %format!("{err:#}"), "failed to receive hub control message");
                    }
                }
            }
            state
                .metrics()
                .record_control_plane(ControlPlaneMetric::ReceiveFailed);
            state.printer_events().invalidate_epoch();
            tracing::error!("hub control plane subscription ended");
            tokio::time::sleep(Duration::from_secs(1)).await;
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
            source_instance_id,
        } => match crate::cluster::parse_agent_identity(&tenant_id, &agent_id) {
            Ok((tenant_id, agent_id)) => {
                if source_instance_id == state.instance_id().to_string() {
                    return;
                }
                if let Some(session) = state
                    .sessions()
                    .close_local_agent(tenant_id, agent_id)
                    .await
                {
                    fail_pending_live_commands(
                        state,
                        tenant_id,
                        agent_id,
                        session,
                        "agent session closed before printer operation completed",
                    )
                    .await;
                }
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
    let expired = state
        .sessions()
        .expire_stale(now, timeout, state.agents())
        .await
        .context("failed to expire stale agent sessions")?;
    let expired_count = expired.len();
    for session in expired {
        let tenant_id = session.tenant_id;
        let agent_id = session.agent_id;
        fail_pending_live_commands(
            state,
            tenant_id,
            agent_id,
            session,
            "agent session expired before printer operation completed",
        )
        .await;
    }
    Ok(expired_count)
}

#[cfg_attr(not(test), allow(dead_code))]
async fn mark_stalled_pending_jobs_once(state: &AppState, now: &str) -> anyhow::Result<usize> {
    let stalled = state
        .jobs()
        .mark_stalled_pending_jobs(now)
        .await
        .context("failed to advance pending print jobs to stalled")?;
    let count = stalled.len();
    for job in stalled {
        let tenant_id = job.job.tenant_id;
        let response = crate::routes::jobs::JobResponse::try_from(job)
            .context("failed to build stalled print job event")?;
        state
            .publish_printer_event(
                tenant_id,
                crate::printer_events::PrinterEvent::JobProgress {
                    job: Box::new(response),
                },
            )
            .await;
    }
    Ok(count)
}

#[cfg_attr(not(test), allow(dead_code))]
async fn fail_stale_live_commands_once(state: &AppState, now: &str) -> anyhow::Result<u64> {
    fail_stale_live_commands_with_timeouts(
        state,
        now,
        STALE_LIVE_COMMAND_TIMEOUT,
        STALE_SESSION_TIMEOUT,
    )
    .await
}

#[cfg_attr(not(test), allow(dead_code))]
async fn fail_stale_live_commands_with_timeouts(
    state: &AppState,
    now: &str,
    command_timeout: Duration,
    session_timeout: Duration,
) -> anyhow::Result<u64> {
    let pending = state.sessions().pending_live_command_ids().await;
    state
        .commands()
        .fail_stale_unowned_live_commands(
            now,
            command_timeout,
            session_timeout,
            state.instance_id(),
            &pending,
        )
        .await
        .context("failed to fail stale unowned live commands")
}

fn log_stale_live_command_cleanup_error(err: &anyhow::Error) {
    tracing::error!(
        error = %crate::redaction::redact_secrets(&format!("{err:#}")),
        "failed to expire stale live commands"
    );
}

#[cfg(test)]
mod tests;
