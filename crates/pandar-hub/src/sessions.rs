use std::{collections::HashMap, future::Future, sync::Arc, time::Duration};

use anyhow::Context;
use pandar_core::{AgentId, TenantId};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::{Mutex, mpsc};
use uuid::Uuid;

use crate::repositories::{AgentRepository, RepositoryError, RepositoryResult};

#[cfg(test)]
use pandar_core::AgentStatus;

#[derive(Debug, Clone)]
pub struct SessionRegistry {
    sessions: Arc<Mutex<HashMap<AgentId, AgentSession>>>,
}

#[derive(Debug, Clone)]
pub struct AgentSession {
    pub token: SessionToken,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub name: String,
    pub version: String,
    pub connected_at: String,
    pub last_heartbeat_at: String,
    pub wake_sender: mpsc::Sender<()>,
    pub close_sender: mpsc::Sender<()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionToken(Uuid);

impl SessionToken {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionToken {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn register(&self, session: AgentSession) -> Option<AgentSession> {
        let previous = self.sessions.lock().await.insert(session.agent_id, session);
        if let Some(previous) = &previous {
            let _ = previous.close_sender.try_send(());
        }
        previous
    }

    pub async fn touch_heartbeat(
        &self,
        agent_id: AgentId,
        observed_at: impl Into<String>,
    ) -> Option<AgentSession> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(&agent_id)?;
        session.last_heartbeat_at = observed_at.into();
        Some(session.clone())
    }

    pub async fn touch_heartbeat_if_current(
        &self,
        agent_id: AgentId,
        token: SessionToken,
        observed_at: impl Into<String>,
    ) -> Option<AgentSession> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(&agent_id)?;
        if session.token != token {
            return None;
        }

        session.last_heartbeat_at = observed_at.into();
        Some(session.clone())
    }

    pub async fn remove(&self, agent_id: AgentId) -> Option<AgentSession> {
        self.sessions.lock().await.remove(&agent_id)
    }

    pub async fn remove_if_current(
        &self,
        agent_id: AgentId,
        token: SessionToken,
    ) -> Option<AgentSession> {
        let mut sessions = self.sessions.lock().await;
        if sessions
            .get(&agent_id)
            .is_some_and(|session| session.token == token)
        {
            return sessions.remove(&agent_id);
        }

        None
    }

    pub async fn count(&self) -> i64 {
        self.sessions
            .lock()
            .await
            .len()
            .try_into()
            .expect("session count should fit in i64")
    }

    pub async fn is_current(&self, agent_id: AgentId, token: SessionToken) -> bool {
        self.sessions
            .lock()
            .await
            .get(&agent_id)
            .is_some_and(|session| session.token == token)
    }

    pub async fn while_current<T, Fut>(
        &self,
        agent_id: AgentId,
        token: SessionToken,
        operation: impl FnOnce() -> Fut,
    ) -> Option<T>
    where
        Fut: Future<Output = T>,
    {
        if !self.is_current(agent_id, token).await {
            return None;
        }

        let result = operation().await;
        self.is_current(agent_id, token).await.then_some(result)
    }

    pub async fn wake_local_agent(&self, tenant_id: TenantId, agent_id: AgentId) {
        let wake_sender = {
            self.sessions
                .lock()
                .await
                .get(&agent_id)
                .filter(|session| session.tenant_id == tenant_id)
                .map(|session| session.wake_sender.clone())
        };

        if let Some(wake_sender) = wake_sender {
            let _ = wake_sender.try_send(());
        }
    }

    pub async fn close_local_agent(&self, tenant_id: TenantId, agent_id: AgentId) {
        let close_sender = {
            let mut sessions = self.sessions.lock().await;
            if sessions
                .get(&agent_id)
                .is_some_and(|session| session.tenant_id == tenant_id)
            {
                sessions
                    .remove(&agent_id)
                    .map(|session| session.close_sender)
            } else {
                None
            }
        };

        if let Some(close_sender) = close_sender {
            let _ = close_sender.try_send(());
        }
    }

    pub async fn expire_stale(
        &self,
        now: &str,
        timeout: Duration,
        agents: &AgentRepository,
    ) -> RepositoryResult<Vec<AgentSession>> {
        let cutoff = cutoff_timestamp(now, timeout)?;
        let stale = {
            let sessions = self.sessions.lock().await;
            sessions
                .values()
                .filter(|session| stale_before(&session.last_heartbeat_at, cutoff))
                .cloned()
                .collect::<Vec<_>>()
        };

        let mut expired = Vec::with_capacity(stale.len());
        for session in stale {
            if self
                .remove_if_current(session.agent_id, session.token)
                .await
                .is_some()
            {
                agents.mark_offline(session.agent_id, now).await?;
                expired.push(session);
            }
        }

        Ok(expired)
    }

    #[cfg(test)]
    pub async fn get(&self, agent_id: AgentId) -> Option<AgentSession> {
        self.sessions.lock().await.get(&agent_id).cloned()
    }
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn cutoff_timestamp(now: &str, timeout: Duration) -> RepositoryResult<OffsetDateTime> {
    let timeout =
        time::Duration::try_from(timeout).context("failed to convert stale session timeout")?;
    OffsetDateTime::parse(now, &Rfc3339)
        .with_context(|| format!("failed to parse stale session timestamp {now}"))
        .map(|now| now - timeout)
        .map_err(RepositoryError::Database)
}

fn stale_before(observed_at: &str, cutoff: OffsetDateTime) -> bool {
    OffsetDateTime::parse(observed_at, &Rfc3339)
        .map(|observed_at| observed_at <= cutoff)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppState;

    #[tokio::test]
    async fn sessions_register_touch_and_remove() {
        let registry = SessionRegistry::new();
        let (wake_sender, _) = mpsc::channel(1);
        let (close_sender, _) = mpsc::channel(1);
        let tenant_id = TenantId::new();
        let agent_id = AgentId::new();

        registry
            .register(AgentSession {
                token: SessionToken::new(),
                tenant_id,
                agent_id,
                name: "agent".to_string(),
                version: "0.1.0".to_string(),
                connected_at: "2026-06-20T00:00:00Z".to_string(),
                last_heartbeat_at: "2026-06-20T00:00:00Z".to_string(),
                wake_sender,
                close_sender,
            })
            .await;

        let touched = registry
            .touch_heartbeat(agent_id, "2026-06-20T00:00:10Z")
            .await
            .unwrap();
        assert_eq!(touched.last_heartbeat_at, "2026-06-20T00:00:10Z");
        assert!(registry.remove(agent_id).await.is_some());
        assert!(registry.get(agent_id).await.is_none());
    }

    #[tokio::test]
    async fn sessions_token_scoped_remove_preserves_replacement() {
        let registry = SessionRegistry::new();
        let tenant_id = TenantId::new();
        let agent_id = AgentId::new();
        let old_token = SessionToken::new();
        let new_token = SessionToken::new();
        let (old_wake_sender, _) = mpsc::channel(1);
        let (old_close_sender, _) = mpsc::channel(1);
        let (new_wake_sender, _) = mpsc::channel(1);
        let (new_close_sender, _) = mpsc::channel(1);

        registry
            .register(AgentSession {
                token: old_token,
                tenant_id,
                agent_id,
                name: "agent".to_string(),
                version: "0.1.0".to_string(),
                connected_at: "2026-06-20T00:00:00Z".to_string(),
                last_heartbeat_at: "2026-06-20T00:00:00Z".to_string(),
                wake_sender: old_wake_sender,
                close_sender: old_close_sender,
            })
            .await;
        registry
            .register(AgentSession {
                token: new_token,
                tenant_id,
                agent_id,
                name: "agent".to_string(),
                version: "0.1.0".to_string(),
                connected_at: "2026-06-20T00:00:10Z".to_string(),
                last_heartbeat_at: "2026-06-20T00:00:10Z".to_string(),
                wake_sender: new_wake_sender,
                close_sender: new_close_sender,
            })
            .await;

        assert!(
            registry
                .remove_if_current(agent_id, old_token)
                .await
                .is_none()
        );
        assert_eq!(registry.get(agent_id).await.unwrap().token, new_token);
    }

    #[tokio::test]
    async fn sessions_close_local_agent_removes_matching_session_only() {
        let registry = SessionRegistry::new();
        let tenant_id = TenantId::new();
        let other_tenant_id = TenantId::new();
        let agent_id = AgentId::new();
        let (wake_sender, _) = mpsc::channel(1);
        let (close_sender, mut close_receiver) = mpsc::channel(1);

        registry
            .register(AgentSession {
                token: SessionToken::new(),
                tenant_id,
                agent_id,
                name: "agent".to_string(),
                version: "0.1.0".to_string(),
                connected_at: "2026-06-20T00:00:00Z".to_string(),
                last_heartbeat_at: "2026-06-20T00:00:00Z".to_string(),
                wake_sender,
                close_sender,
            })
            .await;

        registry.close_local_agent(other_tenant_id, agent_id).await;
        assert!(registry.get(agent_id).await.is_some());

        registry.close_local_agent(tenant_id, agent_id).await;
        assert!(registry.get(agent_id).await.is_none());
        tokio::time::timeout(Duration::from_secs(1), close_receiver.recv())
            .await
            .expect("agent session should receive close")
            .expect("close channel should stay open");
    }

    #[tokio::test]
    async fn sessions_close_local_agent_is_not_blocked_by_in_flight_current_operation() {
        let registry = SessionRegistry::new();
        let tenant_id = TenantId::new();
        let agent_id = AgentId::new();
        let token = SessionToken::new();
        let (wake_sender, _) = mpsc::channel(1);
        let (close_sender, mut close_receiver) = mpsc::channel(1);
        let (operation_started, operation_started_receiver) = tokio::sync::oneshot::channel();
        let (finish_operation, finish_operation_receiver) = tokio::sync::oneshot::channel();

        registry
            .register(AgentSession {
                token,
                tenant_id,
                agent_id,
                name: "agent".to_string(),
                version: "0.1.0".to_string(),
                connected_at: "2026-06-20T00:00:00Z".to_string(),
                last_heartbeat_at: "2026-06-20T00:00:00Z".to_string(),
                wake_sender,
                close_sender,
            })
            .await;

        let operation_registry = registry.clone();
        let operation = tokio::spawn(async move {
            operation_registry
                .while_current(agent_id, token, || async move {
                    let _ = operation_started.send(());
                    let _ = finish_operation_receiver.await;
                    1
                })
                .await
        });
        operation_started_receiver.await.unwrap();

        registry.close_local_agent(tenant_id, agent_id).await;

        assert!(registry.get(agent_id).await.is_none());
        tokio::time::timeout(Duration::from_secs(1), close_receiver.recv())
            .await
            .expect("agent session should receive close")
            .expect("close channel should stay open");
        let _ = finish_operation.send(());
        assert_eq!(operation.await.unwrap(), None);
    }

    #[tokio::test]
    async fn sessions_wake_local_agent_wakes_matching_online_agent() {
        let state = AppState::sqlite_for_tests().await.unwrap();
        let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
        let agent = state.agents().create(tenant.id, "agent").await.unwrap();
        let (wake_sender, mut wake_receiver) = mpsc::channel(1);
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

        let command = state
            .commands()
            .enqueue_refresh_printers(tenant.id, agent.id)
            .await
            .unwrap();
        state.sessions().wake_local_agent(tenant.id, agent.id).await;

        assert_eq!(command.tenant_id, tenant.id);
        assert!(wake_receiver.recv().await.is_some());
    }

    #[tokio::test]
    async fn sessions_expire_stale_marks_agent_offline() {
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

        let expired = state
            .sessions()
            .expire_stale(
                "2026-06-20T00:01:00Z",
                Duration::from_secs(45),
                state.agents(),
            )
            .await
            .unwrap();

        assert_eq!(expired.len(), 1);
        assert!(state.sessions().get(agent.id).await.is_none());
        let persisted = state.agents().get(agent.id).await.unwrap().unwrap();
        assert_eq!(persisted.status, AgentStatus::Offline);
    }
}
