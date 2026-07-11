use std::{
    collections::{HashMap, HashSet},
    future::Future,
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use pandar_core::{AgentId, TenantId};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::{Mutex, OwnedMutexGuard, mpsc};
use tonic::Status;
use uuid::Uuid;

use crate::protocol::agent::v1::{AgentCapability, HubCommand};
use crate::repositories::{AgentRepository, RepositoryError, RepositoryResult};

#[cfg(test)]
use pandar_core::{AgentStatus, CommandId};

pub(crate) mod live_commands;
mod transient;
mod transitions;

#[cfg(test)]
pub(crate) use transitions::pause as transition_pause;

pub use live_commands::{
    LiveCommandClaim, LiveCommandClaimOutcome, LiveDispatchError, PendingLiveCommand,
    PendingLiveCommands, empty_pending_live_commands,
};
use transitions::AgentTransitions;

#[derive(Debug, Clone)]
pub struct SessionRegistry {
    sessions: Arc<Mutex<HashMap<AgentId, AgentSession>>>,
    transitions: AgentTransitions,
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
    pub command_sender: mpsc::Sender<Result<HubCommand, Status>>,
    pub capabilities: HashSet<AgentCapability>,
    pub pending_live_commands: PendingLiveCommands,
    pub live_command_transition: Arc<Mutex<()>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionToken(Uuid);

impl SessionToken {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn persisted_id(self) -> String {
        self.0.to_string()
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
            transitions: AgentTransitions::default(),
        }
    }

    pub async fn transition_lease(&self, agent_id: AgentId) -> OwnedMutexGuard<()> {
        self.transitions.lease(agent_id).await
    }

    pub(crate) async fn transition_lease_for_session(
        &self,
        agent_id: AgentId,
        token: SessionToken,
    ) -> OwnedMutexGuard<()> {
        #[cfg(not(test))]
        let _ = token;
        #[cfg(test)]
        transition_pause::wait_before(token).await;
        #[cfg(test)]
        let lease = self.transitions.lease_observed(agent_id, token).await;
        #[cfg(not(test))]
        let lease = self.transition_lease(agent_id).await;
        #[cfg(test)]
        transition_pause::wait_after(token).await;
        lease
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

    pub async fn current_token(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> Option<SessionToken> {
        self.sessions
            .lock()
            .await
            .get(&agent_id)
            .filter(|session| session.tenant_id == tenant_id)
            .map(|session| session.token)
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

    pub async fn close_local_agent(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> Option<AgentSession> {
        let _lease = self.transition_lease(agent_id).await;
        let removed = {
            let mut sessions = self.sessions.lock().await;
            if sessions
                .get(&agent_id)
                .is_some_and(|session| session.tenant_id == tenant_id)
            {
                sessions.remove(&agent_id)
            } else {
                None
            }
        };

        if let Some(session) = &removed {
            let _ = session.close_sender.try_send(());
        }
        removed
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
        for stale_session in stale {
            let _lease = self
                .transition_lease_for_session(stale_session.agent_id, stale_session.token)
                .await;
            let current = self
                .sessions
                .lock()
                .await
                .get(&stale_session.agent_id)
                .cloned();
            let Some(current) = current.filter(|session| {
                session.token == stale_session.token
                    && stale_before(&session.last_heartbeat_at, cutoff)
            }) else {
                continue;
            };
            agents
                .mark_offline_if_current(
                    current.tenant_id,
                    current.agent_id,
                    &current.token.persisted_id(),
                    now,
                )
                .await?;
            if let Some(session) = self
                .remove_if_current(current.agent_id, current.token)
                .await
            {
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
mod tests;
