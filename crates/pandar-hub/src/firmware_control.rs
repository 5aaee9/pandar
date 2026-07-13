mod execute;
mod execute_cancellation;
#[cfg(test)]
pub(crate) mod execute_ownership_gap_pause;
mod lifecycle;
mod prepare;
mod refresh;
mod types;

#[cfg(test)]
pub(crate) mod dispatch_ownership_pause;

use pandar_core::{AgentId, FirmwareCommand, TenantId};
use sea_orm::DatabaseTransaction;
use tokio::sync::OwnedMutexGuard;

#[cfg(test)]
pub(crate) use lifecycle::finish_pause as lifecycle_finish_pause;
pub(crate) use lifecycle::{
    finish_agent_failure, finish_cancelled_commands, finish_pre_publish_failure,
    finish_unclaimed_pre_publish_failure,
};
pub use types::{
    FirmwareExecutePhase, FirmwareExecuteResult, FirmwareRefreshResult, FirmwareServiceError,
    PreparedFirmwareControl,
};

use crate::{
    AppState,
    protocol::agent::v1::{
        FirmwareCommand as ProtoFirmwareCommand, FirmwareConsistencyConfirm, FirmwareStart,
        FirmwareSwitchAmsFirmware, FirmwareUpgradeConfirm, firmware_command,
    },
    sessions::{FirmwareCommandIdentity, FirmwareSessionDispatch, SessionToken},
};

struct FirmwareTarget {
    tenant_id: TenantId,
    agent_id: AgentId,
    printer_id: String,
    serial: String,
    generation: u64,
    token: SessionToken,
    dispatch: FirmwareSessionDispatch,
    _lease: OwnedMutexGuard<()>,
}

async fn resolve_target(
    state: &AppState,
    tenant_id: TenantId,
    printer_id: &str,
) -> Result<FirmwareTarget, FirmwareServiceError> {
    let printer = state
        .printers()
        .get_with_live_status_for_tenant(tenant_id, printer_id)
        .await
        .map_err(|error| repository_error(error, "failed to load firmware command printer"))?
        .ok_or(FirmwareServiceError::Unavailable)?;
    let agent_id = printer.printer.agent_id;
    let token = state
        .sessions()
        .current_token_for_capability(
            tenant_id,
            agent_id,
            crate::protocol::agent::v1::AgentCapability::FirmwareControl,
        )
        .await
        .ok_or(FirmwareServiceError::Unavailable)?;
    let lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    let printer = state
        .printers()
        .get_with_live_status_for_tenant(tenant_id, printer_id)
        .await
        .map_err(|error| repository_error(error, "failed to reload firmware command printer"))?
        .ok_or(FirmwareServiceError::Unavailable)?;
    let generation = printer
        .firmware
        .generation
        .filter(|_| printer.firmware.session_id.as_deref() == Some(&token.persisted_id()))
        .ok_or(FirmwareServiceError::Unavailable)?;
    let dispatch = state
        .sessions()
        .current_firmware_dispatch(tenant_id, agent_id, token)
        .await
        .ok_or(FirmwareServiceError::Unavailable)?;
    Ok(FirmwareTarget {
        tenant_id,
        agent_id,
        printer_id: printer.printer.id,
        serial: printer.printer.serial_number,
        generation,
        token,
        dispatch,
        _lease: lease,
    })
}

async fn identity_is_current(
    state: &AppState,
    identity: &FirmwareCommandIdentity,
) -> Result<Option<FirmwareSessionDispatch>, FirmwareServiceError> {
    let Some(dispatch) = state
        .sessions()
        .current_firmware_dispatch(
            identity.tenant_id,
            identity.agent_id,
            identity.session_token,
        )
        .await
    else {
        return Ok(None);
    };
    let printer = state
        .printers()
        .get_with_live_status_for_tenant(identity.tenant_id, &identity.printer_id)
        .await
        .map_err(|error| repository_error(error, "failed to validate prepared firmware command"))?;
    let Some(_) = printer.filter(|printer| {
        printer.printer.agent_id == identity.agent_id
            && printer.printer.serial_number == identity.serial
            && printer.firmware.session_id.as_deref()
                == Some(&identity.session_token.persisted_id())
            && printer.firmware.generation == Some(identity.generation)
    }) else {
        return Ok(None);
    };
    let Some(fence) = begin_current_session_fence(
        state,
        identity.tenant_id,
        identity.agent_id,
        identity.session_token,
    )
    .await?
    else {
        return Ok(None);
    };
    fence.commit().await.map_err(|error| {
        FirmwareServiceError::internal(
            anyhow::Error::new(error)
                .context("failed to release prepared firmware ownership validation fence"),
        )
    })?;
    Ok(Some(dispatch))
}

async fn begin_current_session_fence(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
) -> Result<Option<DatabaseTransaction>, FirmwareServiceError> {
    match state
        .agents()
        .begin_current_session_fence(tenant_id, agent_id, &token.persisted_id())
        .await
    {
        Ok(fence) => Ok(Some(fence)),
        Err(
            crate::repositories::RepositoryError::AgentSessionNotCurrent
            | crate::repositories::RepositoryError::MissingAgent,
        ) => Ok(None),
        Err(error) => Err(repository_error(
            error,
            "failed to acquire authoritative firmware session fence",
        )),
    }
}

async fn begin_dispatch_ownership_fence(
    state: &AppState,
    identity: &FirmwareCommandIdentity,
) -> Result<Option<DatabaseTransaction>, FirmwareServiceError> {
    state
        .printers()
        .begin_firmware_dispatch_fence(
            identity.tenant_id,
            identity.agent_id,
            &identity.session_token.persisted_id(),
            &identity.printer_id,
            &identity.serial,
            identity.generation,
        )
        .await
        .map_err(|error| {
            repository_error(
                error,
                "failed to acquire authoritative firmware printer dispatch fence",
            )
        })
}

async fn commit_current_session_fence(
    fence: DatabaseTransaction,
    _pause_key: String,
    context: &'static str,
) -> Result<(), FirmwareServiceError> {
    #[cfg(test)]
    session_fence_commit_pause::wait(&_pause_key).await;
    fence
        .commit()
        .await
        .map_err(|error| FirmwareServiceError::internal(anyhow::Error::new(error).context(context)))
}

fn target_identity(
    target: &FirmwareTarget,
    command_id: pandar_core::CommandId,
) -> FirmwareCommandIdentity {
    FirmwareCommandIdentity {
        command_id,
        tenant_id: target.tenant_id,
        agent_id: target.agent_id,
        session_token: target.token,
        printer_id: target.printer_id.clone(),
        serial: target.serial.clone(),
        generation: target.generation,
    }
}

fn proto_firmware_command(command: FirmwareCommand) -> ProtoFirmwareCommand {
    let (sequence_id, src_id, command) = match command {
        FirmwareCommand::UpgradeConfirm {
            sequence_id,
            src_id,
        } => (
            sequence_id,
            src_id,
            firmware_command::Command::UpgradeConfirm(FirmwareUpgradeConfirm {}),
        ),
        FirmwareCommand::ConsistencyConfirm {
            sequence_id,
            src_id,
        } => (
            sequence_id,
            src_id,
            firmware_command::Command::ConsistencyConfirm(FirmwareConsistencyConfirm {}),
        ),
        FirmwareCommand::Start {
            sequence_id,
            src_id,
            url,
            module,
            version,
        } => (
            sequence_id,
            src_id,
            firmware_command::Command::Start(FirmwareStart {
                url,
                module,
                version,
            }),
        ),
        FirmwareCommand::SwitchAmsFirmware {
            sequence_id,
            src_id,
            id,
        } => (
            sequence_id,
            src_id,
            firmware_command::Command::SwitchAmsFirmware(FirmwareSwitchAmsFirmware { id }),
        ),
    };
    ProtoFirmwareCommand {
        sequence_id,
        src_id,
        command: Some(command),
    }
}

fn repository_error(
    error: crate::repositories::RepositoryError,
    context: &'static str,
) -> FirmwareServiceError {
    FirmwareServiceError::internal(anyhow::Error::new(error).context(context))
}

#[cfg(test)]
pub(crate) mod session_fence_commit_pause {
    use std::{
        collections::HashMap,
        sync::{Mutex, OnceLock},
        time::Duration,
    };

    use tokio::sync::oneshot;

    struct PausePoint {
        reached: oneshot::Sender<()>,
        resume: oneshot::Receiver<()>,
    }

    pub(crate) struct CommitPause {
        reached: oneshot::Receiver<()>,
        resume: Option<oneshot::Sender<()>>,
    }

    pub(crate) fn install(pause_key: &str) -> CommitPause {
        let (reached_sender, reached_receiver) = oneshot::channel();
        let (resume_sender, resume_receiver) = oneshot::channel();
        let previous = pauses()
            .lock()
            .expect("firmware session fence commit pause mutex should not be poisoned")
            .insert(
                pause_key.to_owned(),
                PausePoint {
                    reached: reached_sender,
                    resume: resume_receiver,
                },
            );
        assert!(
            previous.is_none(),
            "firmware fence commit pause already installed"
        );
        CommitPause {
            reached: reached_receiver,
            resume: Some(resume_sender),
        }
    }

    impl CommitPause {
        pub(crate) async fn wait_until_reached(&mut self) {
            tokio::time::timeout(Duration::from_secs(5), &mut self.reached)
                .await
                .expect("timed out waiting for firmware fence commit pause")
                .expect("firmware fence commit pause dropped before being reached");
        }

        pub(crate) fn resume(mut self) {
            let _ = self
                .resume
                .take()
                .expect("firmware fence commit resume sender must be present")
                .send(());
        }
    }

    pub(crate) async fn wait(pause_key: &str) {
        let pause = pauses()
            .lock()
            .expect("firmware session fence commit pause mutex should not be poisoned")
            .remove(pause_key);
        if let Some(pause) = pause {
            let _ = pause.reached.send(());
            let _ = pause.resume.await;
        }
    }

    fn pauses() -> &'static Mutex<HashMap<String, PausePoint>> {
        static PAUSES: OnceLock<Mutex<HashMap<String, PausePoint>>> = OnceLock::new();
        PAUSES.get_or_init(|| Mutex::new(HashMap::new()))
    }
}
