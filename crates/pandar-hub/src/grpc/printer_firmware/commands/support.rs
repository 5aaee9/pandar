use super::*;
use sea_orm::DatabaseTransaction;

pub(super) fn exact_identity(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    command_id: CommandId,
    serial: &str,
    generation: u64,
) -> Option<FirmwareCommandIdentity> {
    state
        .sessions()
        .firmware_command_locator(command_id)
        .filter(|identity| {
            identity_matches_session(identity, tenant_id, agent_id, token)
                && identity.serial == serial
                && identity.generation == generation
        })
}

pub(super) fn identity_matches_session(
    identity: &FirmwareCommandIdentity,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
) -> bool {
    identity.tenant_id == tenant_id
        && identity.agent_id == agent_id
        && identity.session_token == token
}

pub(super) fn core_status(
    status: pandar_protocol::agent::v1::PrinterFirmwareStatus,
) -> CoreFirmwareStatus {
    CoreFirmwareStatus {
        upgrade_state: status.upgrade_state.map(core_upgrade_state),
        cfg: status.cfg,
    }
}

pub(super) async fn begin_current_session_fence(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
) -> Result<Option<DatabaseTransaction>, Status> {
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
        Err(error) => Err(repository_status(error)),
    }
}

pub(super) async fn commit_current_session_fence(
    fence: DatabaseTransaction,
    context: &'static str,
) -> Result<(), Status> {
    fence.commit().await.map_err(|error| {
        repository_status(crate::repositories::RepositoryError::Database(
            anyhow::Error::new(error).context(context),
        ))
    })
}

#[cfg(test)]
pub(crate) mod completion_pause {
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

    pub(crate) struct CompletionPause {
        reached: oneshot::Receiver<()>,
        resume: Option<oneshot::Sender<()>>,
    }

    pub(crate) fn install(command_id: CommandId) -> CompletionPause {
        let (reached_sender, reached_receiver) = oneshot::channel();
        let (resume_sender, resume_receiver) = oneshot::channel();
        let previous = pauses()
            .lock()
            .expect("firmware completion pause mutex should not be poisoned")
            .insert(
                command_id,
                PausePoint {
                    reached: reached_sender,
                    resume: resume_receiver,
                },
            );
        assert!(
            previous.is_none(),
            "firmware completion pause already installed"
        );
        CompletionPause {
            reached: reached_receiver,
            resume: Some(resume_sender),
        }
    }

    impl CompletionPause {
        pub(crate) async fn wait_until_reached(&mut self) {
            tokio::time::timeout(Duration::from_secs(5), &mut self.reached)
                .await
                .expect("timed out waiting for firmware completion pause")
                .expect("firmware completion pause was dropped before being reached");
        }

        pub(crate) fn resume(mut self) {
            let _ = self
                .resume
                .take()
                .expect("firmware completion resume sender must be present")
                .send(());
        }
    }

    pub(crate) async fn wait(command_id: CommandId) {
        let pause = pauses()
            .lock()
            .expect("firmware completion pause mutex should not be poisoned")
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
