use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex as StdMutex},
};

use pandar_core::{AgentId, CommandId, TenantId};
use tokio::sync::{OwnedMutexGuard, mpsc};

use super::{AgentSession, SessionRegistry, SessionToken};
use crate::{
    AppState,
    protocol::agent::v1::{AgentCapability, HubCommand, hub_command},
};

pub type PendingLiveCommands = Arc<StdMutex<HashMap<CommandId, PendingLiveCommand>>>;

#[derive(Debug, Clone)]
pub struct PendingLiveCommand {
    access_code: Option<String>,
}

impl PendingLiveCommand {
    pub fn new(access_code: Option<String>) -> Self {
        Self { access_code }
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

pub enum LiveCommandClaimOutcome {
    Claim(LiveCommandClaim),
    NotCurrent,
    NotPending,
}

pub struct LiveCommandClaim {
    command_id: CommandId,
    pending: PendingLiveCommands,
    access_code: Option<String>,
    _transition: OwnedMutexGuard<()>,
}

impl LiveCommandClaim {
    pub fn access_code(&self) -> Option<&str> {
        self.access_code.as_deref()
    }

    pub fn remove_pending(&self) {
        self.pending
            .lock()
            .expect("pending live commands mutex should not be poisoned")
            .remove(&self.command_id);
    }
}

impl SessionRegistry {
    pub async fn current_token_for_capability(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        capability: AgentCapability,
    ) -> Option<SessionToken> {
        self.sessions
            .lock()
            .await
            .get(&agent_id)
            .filter(|session| {
                session.tenant_id == tenant_id && session.capabilities.contains(&capability)
            })
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
        self.try_dispatch_live_command_inner(tenant_id, agent_id, token, None, command_id, command)
            .await
    }

    pub async fn try_dispatch_live_command_with_capability(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        token: SessionToken,
        capability: AgentCapability,
        command_id: CommandId,
        command: HubCommand,
    ) -> Result<(), LiveDispatchError> {
        self.try_dispatch_live_command_inner(
            tenant_id,
            agent_id,
            token,
            Some(capability),
            command_id,
            command,
        )
        .await
    }

    async fn try_dispatch_live_command_inner(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        token: SessionToken,
        capability: Option<AgentCapability>,
        command_id: CommandId,
        command: HubCommand,
    ) -> Result<(), LiveDispatchError> {
        let sessions = self.sessions.lock().await;
        let Some(session) = sessions.get(&agent_id).filter(|session| {
            session.tenant_id == tenant_id
                && session.token == token
                && capability.is_none_or(|capability| session.capabilities.contains(&capability))
        }) else {
            return Err(LiveDispatchError::NotCurrent);
        };

        session
            .pending_live_commands
            .lock()
            .expect("pending live commands mutex should not be poisoned")
            .insert(
                command_id,
                PendingLiveCommand::new(live_command_access_code(&command)),
            );
        match session.command_sender.try_send(Ok(command)) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Closed(_)) => {
                remove_pending(&session.pending_live_commands, command_id);
                Err(LiveDispatchError::ChannelClosed)
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                remove_pending(&session.pending_live_commands, command_id);
                Err(LiveDispatchError::ChannelFull)
            }
        }
    }

    pub async fn claim_current_live_command(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        token: SessionToken,
        command_id: CommandId,
    ) -> LiveCommandClaimOutcome {
        let sessions = self.sessions.lock().await;
        let Some(session) = sessions
            .get(&agent_id)
            .filter(|session| session.tenant_id == tenant_id && session.token == token)
        else {
            return LiveCommandClaimOutcome::NotCurrent;
        };
        let pending = session.pending_live_commands.clone();
        if !pending
            .lock()
            .expect("pending live commands mutex should not be poisoned")
            .contains_key(&command_id)
        {
            return LiveCommandClaimOutcome::NotPending;
        }

        let transition = session.live_command_transition.clone().lock_owned().await;
        let access_code = {
            let pending = pending
                .lock()
                .expect("pending live commands mutex should not be poisoned");
            let Some(command) = pending.get(&command_id) else {
                return LiveCommandClaimOutcome::NotPending;
            };
            command.access_code.clone()
        };
        drop(sessions);

        LiveCommandClaimOutcome::Claim(LiveCommandClaim {
            command_id,
            pending,
            access_code,
            _transition: transition,
        })
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
        pending.extend(self.pending_firmware_command_ids());
        pending.into_iter().collect()
    }
}

pub(crate) async fn fail_pending_live_commands(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    session: AgentSession,
    reason: &'static str,
) {
    let firmware = {
        let _lease = state
            .sessions()
            .transition_lease_for_session(agent_id, session.token)
            .await;
        state
            .sessions()
            .cancel_firmware_session_under_transition(agent_id, session.token)
    };
    crate::firmware_control::finish_cancelled_commands(state, firmware, reason).await;
    let _transition = session.live_command_transition.clone().lock_owned().await;
    let command_ids = session
        .pending_live_commands
        .lock()
        .expect("pending live commands mutex should not be poisoned")
        .drain()
        .map(|(command_id, _)| command_id)
        .collect::<Vec<_>>();
    for command_id in command_ids {
        if let Err(err) = state
            .commands()
            .mark_failed(command_id, tenant_id, agent_id, reason)
            .await
        {
            tracing::error!(
                command_id = %command_id,
                error = %crate::redaction::redact_secrets(&format!("{err:#}")),
                "failed to fail pending live command after agent session removal"
            );
        }
    }
}

impl AppState {
    pub async fn close_agent(&self, tenant_id: TenantId, agent_id: AgentId) {
        self.camera_sessions().close_agent(agent_id).await;
        if let Some(session) = self.sessions().close_local_agent(tenant_id, agent_id).await {
            fail_pending_live_commands(
                self,
                tenant_id,
                agent_id,
                session,
                "agent session closed before printer operation completed",
            )
            .await;
            self.publish_agent_printers_projection_changes(tenant_id, agent_id)
                .await;
        }
        if let Err(err) = self
            .control_plane()
            .publish(crate::cluster::HubControlMessage::AgentClose {
                tenant_id: tenant_id.to_string(),
                agent_id: agent_id.to_string(),
                source_instance_id: self.instance_id().to_string(),
            })
            .await
        {
            self.metrics()
                .record_control_plane(crate::metrics::ControlPlaneMetric::PublishFailed);
            tracing::error!(error = %format!("{err:#}"), "failed to publish agent close control message");
        } else {
            self.metrics()
                .record_control_plane(crate::metrics::ControlPlaneMetric::PublishOk);
        }
    }
}

fn live_command_access_code(command: &HubCommand) -> Option<String> {
    match command.command.as_ref()? {
        hub_command::Command::LinkPrinter(command) => Some(command.access_code.clone()),
        _ => None,
    }
}

fn remove_pending(pending: &PendingLiveCommands, command_id: CommandId) {
    pending
        .lock()
        .expect("pending live commands mutex should not be poisoned")
        .remove(&command_id);
}
