use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::{Arc, Mutex as StdMutex},
};

use pandar_core::{AgentId, CommandId, FirmwareCommand, FirmwareControlMetadata, TenantId};
use tokio::sync::{mpsc, oneshot};

use super::{SessionRegistry, SessionToken};
use crate::firmware_control::{FirmwareExecuteResult, FirmwareRefreshResult, FirmwareServiceError};
use pandar_protocol::agent::v1::{AgentCapability, HubCommand};

mod execute;
mod lookup;
mod secret;
mod store;
#[cfg(test)]
mod zeroization;

use secret::FirmwareSecret;

pub(crate) type PrepareWaiter = oneshot::Sender<Result<(), FirmwareServiceError>>;
pub(crate) type RefreshWaiter =
    oneshot::Sender<Result<FirmwareRefreshResult, FirmwareServiceError>>;
pub(crate) type ExecuteWaiter = oneshot::Sender<FirmwareExecuteResult>;

#[derive(Clone, Default)]
pub(crate) struct PendingFirmwareCommands {
    inner: Arc<StdMutex<PendingFirmwareState>>,
    completing: Arc<StdMutex<HashSet<CommandId>>>,
}

impl fmt::Debug for PendingFirmwareCommands {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self
            .inner
            .lock()
            .expect("pending firmware commands mutex should not be poisoned")
            .commands
            .len()
            + self
                .completing
                .lock()
                .expect("completing firmware commands mutex should not be poisoned")
                .len();
        formatter
            .debug_struct("PendingFirmwareCommands")
            .field("count", &count)
            .finish()
    }
}

#[derive(Default)]
struct PendingFirmwareState {
    commands: HashMap<CommandId, PendingFirmwareCommand>,
    prepared_tokens: HashMap<FirmwareSecret, CommandId>,
    retained_redaction_urls: HashMap<RetainedFirmwareScope, Vec<FirmwareSecret>>,
}

/// Retained redaction URLs are indexed per tenant and serial so every
/// redaction only scans the URLs observed for that one printer scope.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RetainedFirmwareScope {
    tenant_id: TenantId,
    serial: String,
}

struct PendingFirmwareCommand {
    identity: FirmwareCommandIdentity,
    kind: PendingFirmwareKind,
    phase: PendingFirmwarePhase,
    prepared_token: Option<FirmwareSecret>,
    expires_at: Option<tokio::time::Instant>,
    transient_url: Option<FirmwareSecret>,
    prepare_waiter: Option<PrepareWaiter>,
    refresh_waiter: Option<RefreshWaiter>,
    execute_waiter: Option<ExecuteWaiter>,
}

enum PendingFirmwareKind {
    Refresh,
    Control(FirmwareControlMetadata),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FirmwareCommandIdentity {
    pub command_id: CommandId,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub session_token: SessionToken,
    pub printer_id: String,
    pub serial: String,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PendingFirmwarePhase {
    RefreshSent,
    Preparing,
    Prepared,
    ExecuteSent,
    Published,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClaimedFirmwareKind {
    Refresh,
    Control,
}

pub(crate) struct ClaimedFirmwareCommand {
    pub identity: FirmwareCommandIdentity,
    pub kind: ClaimedFirmwareKind,
    pub phase: PendingFirmwarePhase,
    pub prepare_waiter: Option<PrepareWaiter>,
    pub refresh_waiter: Option<RefreshWaiter>,
    pub execute_waiter: Option<ExecuteWaiter>,
    pub redacted_error: Option<String>,
    _completion_ownership: Option<FirmwareCompletionOwnership>,
}

struct FirmwareCompletionOwnership {
    command_id: CommandId,
    completing: Arc<StdMutex<HashSet<CommandId>>>,
}

impl Drop for FirmwareCompletionOwnership {
    fn drop(&mut self) {
        self.completing
            .lock()
            .expect("completing firmware commands mutex should not be poisoned")
            .remove(&self.command_id);
    }
}

#[derive(Clone)]
pub(crate) struct FirmwareSessionDispatch {
    pub command_sender: mpsc::Sender<Result<HubCommand, tonic::Status>>,
}

impl SessionRegistry {
    pub(crate) async fn current_firmware_dispatch(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        token: SessionToken,
    ) -> Option<FirmwareSessionDispatch> {
        self.sessions
            .lock()
            .await
            .get(&agent_id)
            .filter(|session| {
                session.tenant_id == tenant_id
                    && session.token == token
                    && session
                        .capabilities
                        .contains(&AgentCapability::FirmwareControl)
            })
            .map(|session| FirmwareSessionDispatch {
                command_sender: session.command_sender.clone(),
            })
    }

    pub(crate) fn begin_firmware_refresh_under_transition(
        &self,
        identity: FirmwareCommandIdentity,
        waiter: RefreshWaiter,
    ) {
        self.firmware_commands.insert(PendingFirmwareCommand {
            identity,
            kind: PendingFirmwareKind::Refresh,
            phase: PendingFirmwarePhase::RefreshSent,
            prepared_token: None,
            expires_at: None,
            transient_url: None,
            prepare_waiter: None,
            refresh_waiter: Some(waiter),
            execute_waiter: None,
        });
    }

    pub(crate) fn begin_firmware_prepare_under_transition(
        &self,
        identity: FirmwareCommandIdentity,
        metadata: FirmwareControlMetadata,
        expires_at: tokio::time::Instant,
        waiter: PrepareWaiter,
    ) -> String {
        let prepared_token = uuid::Uuid::new_v4().to_string();
        self.firmware_commands.insert(PendingFirmwareCommand {
            identity,
            kind: PendingFirmwareKind::Control(metadata),
            phase: PendingFirmwarePhase::Preparing,
            prepared_token: Some(FirmwareSecret::from(prepared_token.clone())),
            expires_at: Some(expires_at),
            transient_url: None,
            prepare_waiter: Some(waiter),
            refresh_waiter: None,
            execute_waiter: None,
        });
        prepared_token
    }

    pub(crate) fn complete_firmware_prepared_under_transition(
        &self,
        identity: &FirmwareCommandIdentity,
    ) -> bool {
        let mut state = self.firmware_commands.lock();
        let Some(command) = state.commands.get_mut(&identity.command_id) else {
            return false;
        };
        if command.identity != *identity
            || command.phase != PendingFirmwarePhase::Preparing
            || prepare_expired(command)
        {
            return false;
        }
        command.phase = PendingFirmwarePhase::Prepared;
        if let Some(waiter) = command.prepare_waiter.take() {
            let _ = waiter.send(Ok(()));
        }
        true
    }

    pub(crate) fn claim_firmware_result_under_transition(
        &self,
        identity: &FirmwareCommandIdentity,
        error: Option<&str>,
    ) -> Option<ClaimedFirmwareCommand> {
        self.firmware_commands.remove_exact(identity, error)
    }

    pub(crate) fn claim_firmware_typed_result_under_transition(
        &self,
        identity: &FirmwareCommandIdentity,
    ) -> Option<ClaimedFirmwareCommand> {
        let mut state = self.firmware_commands.lock();
        let command = state.commands.get(&identity.command_id)?;
        if command.identity != *identity
            || !matches!(
                (&command.kind, command.phase),
                (
                    PendingFirmwareKind::Refresh,
                    PendingFirmwarePhase::RefreshSent
                ) | (
                    PendingFirmwareKind::Control(_),
                    PendingFirmwarePhase::ExecuteSent | PendingFirmwarePhase::Published
                )
            )
        {
            return None;
        }
        let mut claimed = store::remove_command(&mut state, identity.command_id, None);
        claimed._completion_ownership =
            Some(self.firmware_commands.begin_completion(identity.command_id));
        drop(state);
        Some(claimed)
    }

    pub(crate) fn expire_firmware_prepare_under_transition(
        &self,
        identity: &FirmwareCommandIdentity,
        now: tokio::time::Instant,
    ) -> Option<ClaimedFirmwareCommand> {
        let state = self.firmware_commands.lock();
        let command = state.commands.get(&identity.command_id)?;
        if command.identity != *identity
            || command.expires_at.is_none_or(|expires_at| expires_at > now)
            || !matches!(
                command.phase,
                PendingFirmwarePhase::Preparing | PendingFirmwarePhase::Prepared
            )
        {
            return None;
        }
        drop(state);
        self.firmware_commands.remove_exact(identity, None)
    }

    pub(crate) fn cancel_firmware_session_under_transition(
        &self,
        agent_id: AgentId,
        token: SessionToken,
    ) -> Vec<ClaimedFirmwareCommand> {
        self.firmware_commands.remove_matching(|identity| {
            identity.agent_id == agent_id && identity.session_token == token
        })
    }

    pub(crate) fn cancel_firmware_generation_under_transition(
        &self,
        agent_id: AgentId,
        token: SessionToken,
        serial: &str,
        new_generation: u64,
    ) -> Vec<ClaimedFirmwareCommand> {
        self.firmware_commands.remove_matching(|identity| {
            identity.agent_id == agent_id
                && identity.session_token == token
                && identity.serial == serial
                && identity.generation != new_generation
        })
    }
}

fn prepare_expired(command: &PendingFirmwareCommand) -> bool {
    command
        .expires_at
        .is_some_and(|expires_at| expires_at <= tokio::time::Instant::now())
}
