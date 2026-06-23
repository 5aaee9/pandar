use std::time::Duration;

use anyhow::Context;
use futures_util::StreamExt;
use tokio::{sync::oneshot, task::JoinHandle};

use crate::{AppState, cluster::HubControlMessage};

const STALE_SESSION_TIMEOUT: Duration = Duration::from_secs(45);
const STALE_SESSION_SWEEP_INTERVAL: Duration = Duration::from_secs(15);

pub fn spawn_session_expiry(state: AppState) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(STALE_SESSION_SWEEP_INTERVAL);
        loop {
            ticker.tick().await;
            if let Err(err) =
                expire_stale_sessions_once(&state, &pandar_core::created_at_now()).await
            {
                tracing::error!(error = %format!("{err:#}"), "failed to expire stale agent sessions");
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
                Ok(message) => handle_control_message(&state, message).await,
                Err(err) => {
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

#[cfg(test)]
mod tests {
    use pandar_core::{AgentId, AgentStatus, TenantId};
    use tokio::sync::mpsc;

    use super::*;
    use crate::sessions::{AgentSession, SessionToken};

    #[tokio::test]
    async fn runtime_expiry_tick_marks_stale_agent_offline() {
        let state = AppState::sqlite_for_tests().await.unwrap();
        let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
        let agent = state.agents().create(tenant.id, "agent").await.unwrap();
        state
            .agents()
            .update_connection(
                agent.id,
                AgentStatus::Online,
                Some("0.1.0"),
                "2026-06-20T00:00:00Z",
            )
            .await
            .unwrap();
        let (wake_sender, _) = mpsc::channel(1);
        let (close_sender, _) = mpsc::channel(1);
        state
            .sessions()
            .register(AgentSession {
                token: SessionToken::new(),
                tenant_id: tenant.id,
                agent_id: agent.id,
                name: agent.name,
                version: "0.1.0".to_string(),
                connected_at: "2026-06-20T00:00:00Z".to_string(),
                last_heartbeat_at: "2026-06-20T00:00:00Z".to_string(),
                wake_sender,
                close_sender,
            })
            .await;

        let expired = expire_stale_sessions_with_timeout(
            &state,
            "2026-06-20T00:00:10Z",
            Duration::from_secs(5),
        )
        .await
        .unwrap();

        assert_eq!(expired, 1);
        assert!(state.sessions().get(agent.id).await.is_none());
        let persisted = state.agents().get(agent.id).await.unwrap().unwrap();
        assert_eq!(persisted.status, AgentStatus::Offline);
    }

    #[tokio::test]
    async fn sibling_instance_can_wake_connected_agent() {
        let state = AppState::sqlite_for_tests().await.unwrap();
        let sibling = state.sibling_for_tests();
        let (_control_plane, ready) = spawn_control_plane_ready(sibling.clone());
        ready.await.unwrap().unwrap();
        let tenant = state
            .tenants()
            .create("wake-acme", "Wake Acme")
            .await
            .unwrap();
        let agent = state
            .agents()
            .create(tenant.id, "wake-agent")
            .await
            .unwrap();
        let (mut wake_receiver, _close_receiver) =
            register_test_session(&sibling, tenant.id, agent.id, "wake-agent").await;

        state.wake_agent(tenant.id, agent.id).await;

        tokio::time::timeout(Duration::from_secs(1), wake_receiver.recv())
            .await
            .expect("sibling agent should receive wake")
            .expect("wake channel should stay open");
    }

    #[tokio::test]
    async fn sibling_agent_wake_ignores_wrong_tenant_and_agent() {
        let state = AppState::sqlite_for_tests().await.unwrap();
        let sibling = state.sibling_for_tests();
        let (_control_plane, ready) = spawn_control_plane_ready(sibling.clone());
        ready.await.unwrap().unwrap();
        let tenant = state
            .tenants()
            .create("wrong-wake-acme", "Wrong Wake Acme")
            .await
            .unwrap();
        let agent = state
            .agents()
            .create(tenant.id, "wrong-wake-agent")
            .await
            .unwrap();
        let (mut wake_receiver, _close_receiver) =
            register_test_session(&sibling, tenant.id, agent.id, "wrong-wake-agent").await;

        state.wake_agent(TenantId::new(), agent.id).await;
        state.wake_agent(tenant.id, AgentId::new()).await;

        assert!(
            tokio::time::timeout(Duration::from_millis(100), wake_receiver.recv())
                .await
                .is_err(),
            "wrong tenant or agent must not wake the sibling session"
        );
    }

    #[tokio::test]
    async fn sibling_instance_can_close_connected_agent() {
        let state = AppState::sqlite_for_tests().await.unwrap();
        let sibling = state.sibling_for_tests();
        let (_control_plane, ready) = spawn_control_plane_ready(sibling.clone());
        ready.await.unwrap().unwrap();
        let tenant = state
            .tenants()
            .create("close-acme", "Close Acme")
            .await
            .unwrap();
        let agent = state
            .agents()
            .create(tenant.id, "close-agent")
            .await
            .unwrap();
        let (_wake_receiver, mut close_receiver) =
            register_test_session(&sibling, tenant.id, agent.id, "close-agent").await;

        state.close_agent(tenant.id, agent.id).await;

        tokio::time::timeout(Duration::from_secs(1), close_receiver.recv())
            .await
            .expect("sibling agent should receive close")
            .expect("close channel should stay open");
    }

    #[tokio::test]
    async fn sibling_agent_close_ignores_wrong_tenant_and_agent() {
        let state = AppState::sqlite_for_tests().await.unwrap();
        let sibling = state.sibling_for_tests();
        let (_control_plane, ready) = spawn_control_plane_ready(sibling.clone());
        ready.await.unwrap().unwrap();
        let tenant = state
            .tenants()
            .create("wrong-close-acme", "Wrong Close Acme")
            .await
            .unwrap();
        let agent = state
            .agents()
            .create(tenant.id, "wrong-close-agent")
            .await
            .unwrap();
        let (_wake_receiver, mut close_receiver) =
            register_test_session(&sibling, tenant.id, agent.id, "wrong-close-agent").await;

        state.close_agent(TenantId::new(), agent.id).await;
        state.close_agent(tenant.id, AgentId::new()).await;

        assert!(
            tokio::time::timeout(Duration::from_millis(100), close_receiver.recv())
                .await
                .is_err(),
            "wrong tenant or agent must not close the sibling session"
        );
    }

    async fn register_test_session(
        state: &AppState,
        tenant_id: TenantId,
        agent_id: AgentId,
        name: &str,
    ) -> (mpsc::Receiver<()>, mpsc::Receiver<()>) {
        let (wake_sender, wake_receiver) = mpsc::channel(1);
        let (close_sender, close_receiver) = mpsc::channel(1);
        state
            .sessions()
            .register(AgentSession {
                token: SessionToken::new(),
                tenant_id,
                agent_id,
                name: name.to_owned(),
                version: "0.1.0".to_owned(),
                connected_at: pandar_core::created_at_now(),
                last_heartbeat_at: pandar_core::created_at_now(),
                wake_sender,
                close_sender,
            })
            .await;
        (wake_receiver, close_receiver)
    }
}
