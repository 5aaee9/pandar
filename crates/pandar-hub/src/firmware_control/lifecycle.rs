use pandar_core::{AgentId, CommandId, CommandStatus, TenantId};

use super::{FirmwareExecutePhase, FirmwareExecuteResult, FirmwareServiceError};
use crate::{
    AppState,
    repositories::{FirmwarePersistedPhase, FirmwarePersistedResult},
    sessions::{ClaimedFirmwareCommand, PendingFirmwarePhase},
};

pub(crate) async fn finish_pre_publish_failure(
    state: &AppState,
    claimed: ClaimedFirmwareCommand,
    reason: &'static str,
) {
    finish(
        state,
        claimed,
        reason,
        FirmwareExecutePhase::PrePublishFailure,
    )
    .await;
}

pub(crate) async fn finish_unclaimed_pre_publish_failure(
    state: &AppState,
    command_id: CommandId,
    tenant_id: TenantId,
    agent_id: AgentId,
    reason: &'static str,
) -> Result<(), FirmwareServiceError> {
    state
        .commands()
        .mark_firmware_terminal(
            command_id,
            tenant_id,
            agent_id,
            CommandStatus::Failed,
            Some(reason.to_owned()),
            FirmwarePersistedResult {
                phase: FirmwarePersistedPhase::PrePublishFailure,
                outcome: None,
                transient_status: None,
            },
        )
        .await
        .map(|_| ())
        .map_err(|error| {
            FirmwareServiceError::internal(
                anyhow::Error::new(error)
                    .context("failed to persist undispatched firmware command failure"),
            )
        })
}

pub(crate) async fn finish_agent_failure(
    state: &AppState,
    claimed: ClaimedFirmwareCommand,
    reason: &'static str,
) {
    let phase = if claimed.phase == PendingFirmwarePhase::Published {
        FirmwareExecutePhase::OutcomeUnknown
    } else {
        FirmwareExecutePhase::PrePublishFailure
    };
    finish(state, claimed, reason, phase).await;
}

pub(crate) async fn finish_cancelled_commands(
    state: &AppState,
    claimed: Vec<ClaimedFirmwareCommand>,
    reason: &'static str,
) {
    for command in claimed {
        let phase = if matches!(
            command.phase,
            PendingFirmwarePhase::ExecuteSent | PendingFirmwarePhase::Published
        ) {
            FirmwareExecutePhase::OutcomeUnknown
        } else {
            FirmwareExecutePhase::PrePublishFailure
        };
        finish(state, command, reason, phase).await;
    }
}

async fn finish(
    state: &AppState,
    mut claimed: ClaimedFirmwareCommand,
    fallback_reason: &'static str,
    execute_phase: FirmwareExecutePhase,
) {
    #[cfg(test)]
    finish_pause::wait(claimed.identity.command_id).await;
    let message = claimed
        .redacted_error
        .take()
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| fallback_reason.to_owned());
    let persisted_phase = match execute_phase {
        FirmwareExecutePhase::PrePublishFailure => FirmwarePersistedPhase::PrePublishFailure,
        FirmwareExecutePhase::OutcomeUnknown => FirmwarePersistedPhase::OutcomeUnknown,
        FirmwareExecutePhase::Acknowledged => FirmwarePersistedPhase::Acknowledged,
        FirmwareExecutePhase::Rejected => FirmwarePersistedPhase::Rejected,
    };
    let persisted = match state
        .commands()
        .mark_firmware_terminal(
            claimed.identity.command_id,
            claimed.identity.tenant_id,
            claimed.identity.agent_id,
            CommandStatus::Failed,
            Some(message.clone()),
            FirmwarePersistedResult {
                phase: persisted_phase,
                outcome: None,
                transient_status: None,
            },
        )
        .await
    {
        Ok(_) => true,
        Err(error) => {
            tracing::error!(
                command_id = %claimed.identity.command_id,
                error = %format!("{:#}", anyhow::Error::new(error).context("failed to persist firmware lifecycle terminal phase")),
                "failed to persist firmware lifecycle cleanup"
            );
            false
        }
    };
    if let Some(waiter) = claimed.prepare_waiter.take() {
        let _ = waiter.send(Err(FirmwareServiceError::CommandFailed {
            message: message.clone(),
        }));
    }
    if let Some(waiter) = claimed.refresh_waiter.take() {
        let _ = waiter.send(Err(FirmwareServiceError::CommandFailed {
            message: message.clone(),
        }));
    }
    if let Some(waiter) = claimed.execute_waiter.take() {
        let _ = waiter.send(FirmwareExecuteResult {
            command_id: claimed.identity.command_id,
            phase: if persisted {
                execute_phase
            } else {
                FirmwareExecutePhase::OutcomeUnknown
            },
            outcome: None,
            transient_status: None,
            error: Some(message),
        });
    }
}

#[cfg(test)]
pub(crate) mod finish_pause {
    use std::{
        collections::HashMap,
        sync::{Mutex, OnceLock},
        time::Duration,
    };

    use pandar_core::CommandId;
    use tokio::sync::oneshot;

    struct PausePoint {
        reached: oneshot::Sender<()>,
        resume: oneshot::Receiver<()>,
    }

    pub(crate) struct FinishPause {
        reached: oneshot::Receiver<()>,
        resume: Option<oneshot::Sender<()>>,
    }

    pub(crate) fn install(command_id: CommandId) -> FinishPause {
        let (reached_sender, reached_receiver) = oneshot::channel();
        let (resume_sender, resume_receiver) = oneshot::channel();
        let previous = pauses()
            .lock()
            .expect("firmware finish pause mutex should not be poisoned")
            .insert(
                command_id,
                PausePoint {
                    reached: reached_sender,
                    resume: resume_receiver,
                },
            );
        assert!(
            previous.is_none(),
            "firmware finish pause already installed"
        );
        FinishPause {
            reached: reached_receiver,
            resume: Some(resume_sender),
        }
    }

    impl FinishPause {
        pub(crate) async fn wait_until_reached(&mut self) {
            tokio::time::timeout(Duration::from_secs(5), &mut self.reached)
                .await
                .expect("timed out waiting for firmware finish pause")
                .expect("firmware finish pause was dropped before being reached");
        }

        pub(crate) fn resume(mut self) {
            let _ = self
                .resume
                .take()
                .expect("firmware finish resume sender must be present")
                .send(());
        }
    }

    pub(crate) async fn wait(command_id: CommandId) {
        let pause = pauses()
            .lock()
            .expect("firmware finish pause mutex should not be poisoned")
            .remove(&command_id);
        if let Some(pause) = pause {
            let _ = pause.reached.send(());
            let _ = pause.resume.await;
        }
    }

    fn pauses() -> &'static Mutex<HashMap<CommandId, PausePoint>> {
        static PAUSES: OnceLock<Mutex<HashMap<CommandId, PausePoint>>> = OnceLock::new();
        PAUSES.get_or_init(|| Mutex::new(HashMap::new()))
    }
}
