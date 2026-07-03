use std::{
    collections::{HashMap, HashSet},
    future::Future,
    sync::{Arc, Mutex as StdMutex},
    time::Duration,
};

use anyhow::Context;
use pandar_core::{AgentId, CommandId, TenantId};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::{Mutex, mpsc};
use tonic::Status;
use uuid::Uuid;

use crate::protocol::agent::v1::HubCommand;
use crate::protocol::agent::v1::hub_command;
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
    pub command_sender: mpsc::Sender<Result<HubCommand, Status>>,
    pub pending_live_commands: PendingLiveCommands,
}

pub type PendingLiveCommands = Arc<StdMutex<HashMap<CommandId, PendingLiveCommand>>>;

#[derive(Debug, Clone)]
pub struct PendingLiveCommand {
    access_code: Option<String>,
}

impl PendingLiveCommand {
    pub fn new(access_code: Option<String>) -> Self {
        Self { access_code }
    }

    pub fn access_code(&self) -> Option<&str> {
        self.access_code.as_deref()
    }
}

pub fn empty_pending_live_commands() -> PendingLiveCommands {
    Arc::new(StdMutex::new(HashMap::new()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveDispatchError {
    NotCurrent,
    ChannelClosed,
    ChannelFull,
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

    pub async fn try_dispatch_live_command(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        token: SessionToken,
        command_id: CommandId,
        command: HubCommand,
    ) -> Result<(), LiveDispatchError> {
        let sessions = self.sessions.lock().await;
        let Some(session) = sessions
            .get(&agent_id)
            .filter(|session| session.tenant_id == tenant_id && session.token == token)
        else {
            return Err(LiveDispatchError::NotCurrent);
        };

        let pending = PendingLiveCommand::new(live_command_access_code(&command));
        session
            .pending_live_commands
            .lock()
            .expect("pending live commands mutex should not be poisoned")
            .insert(command_id, pending);
        match session.command_sender.try_send(Ok(command)) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Closed(_)) => {
                session
                    .pending_live_commands
                    .lock()
                    .expect("pending live commands mutex should not be poisoned")
                    .remove(&command_id);
                Err(LiveDispatchError::ChannelClosed)
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                session
                    .pending_live_commands
                    .lock()
                    .expect("pending live commands mutex should not be poisoned")
                    .remove(&command_id);
                Err(LiveDispatchError::ChannelFull)
            }
        }
    }

    pub async fn pending_live_command_ids(&self) -> Vec<CommandId> {
        let sessions = self.sessions.lock().await;
        let mut pending = HashSet::new();
        for session in sessions.values() {
            pending.extend(
                session
                    .pending_live_commands
                    .lock()
                    .expect("pending live commands mutex should not be poisoned")
                    .keys()
                    .copied(),
            );
        }
        pending.into_iter().collect()
    }

    pub async fn pending_live_command_access_code(
        &self,
        agent_id: AgentId,
        token: SessionToken,
        command_id: CommandId,
    ) -> Option<String> {
        let sessions = self.sessions.lock().await;
        let session = sessions
            .get(&agent_id)
            .filter(|session| session.token == token)?;

        session
            .pending_live_commands
            .lock()
            .expect("pending live commands mutex should not be poisoned")
            .get(&command_id)
            .and_then(|pending| pending.access_code().map(ToOwned::to_owned))
    }

    pub async fn remove_pending_live_command(
        &self,
        agent_id: AgentId,
        token: SessionToken,
        command_id: CommandId,
    ) -> bool {
        let sessions = self.sessions.lock().await;
        let Some(session) = sessions
            .get(&agent_id)
            .filter(|session| session.token == token)
        else {
            return false;
        };

        session
            .pending_live_commands
            .lock()
            .expect("pending live commands mutex should not be poisoned")
            .remove(&command_id)
            .is_some()
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

fn live_command_access_code(command: &HubCommand) -> Option<String> {
    match command.command.as_ref()? {
        hub_command::Command::LinkPrinter(command) => Some(command.access_code.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
