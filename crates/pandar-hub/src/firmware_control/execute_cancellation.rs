use tokio::task::JoinHandle;

use crate::{AppState, sessions::FirmwareCommandIdentity};

#[derive(Clone, Copy)]
enum CancellationPhase {
    PrePublish,
    OutcomeUnknown,
}

pub(super) struct ExecuteCancellationOwner {
    state: AppState,
    identity: FirmwareCommandIdentity,
    phase: CancellationPhase,
    armed: bool,
}

impl ExecuteCancellationOwner {
    pub(super) fn new(state: &AppState, identity: FirmwareCommandIdentity) -> Self {
        Self {
            state: state.clone(),
            identity,
            phase: CancellationPhase::PrePublish,
            armed: true,
        }
    }

    pub(super) fn mark_dispatch_attempted(&mut self) {
        self.phase = CancellationPhase::OutcomeUnknown;
    }

    pub(super) fn schedule_pre_publish(&mut self, reason: &'static str) -> Option<JoinHandle<()>> {
        self.schedule(reason, CancellationPhase::PrePublish)
    }

    pub(super) fn schedule_pre_publish_error(&mut self, error: &str) -> Option<JoinHandle<()>> {
        let claimed = self.claim(Some(error))?;
        Some(spawn_cleanup(
            self.state.clone(),
            claimed,
            "firmware execute could not be dispatched",
            CancellationPhase::PrePublish,
        ))
    }

    pub(super) fn schedule_outcome_unknown(
        &mut self,
        reason: &'static str,
    ) -> Option<JoinHandle<()>> {
        self.schedule(reason, CancellationPhase::OutcomeUnknown)
    }

    pub(super) fn disarm(&mut self) {
        self.armed = false;
    }

    fn schedule(
        &mut self,
        reason: &'static str,
        phase: CancellationPhase,
    ) -> Option<JoinHandle<()>> {
        let claimed = self.claim(None)?;
        Some(spawn_cleanup(self.state.clone(), claimed, reason, phase))
    }

    fn claim(&mut self, error: Option<&str>) -> Option<crate::sessions::ClaimedFirmwareCommand> {
        if !self.armed {
            return None;
        }
        self.armed = false;
        self.state
            .sessions()
            .claim_firmware_result_under_transition(&self.identity, error)
    }
}

impl Drop for ExecuteCancellationOwner {
    fn drop(&mut self) {
        let phase = self.phase;
        let Some(claimed) = self.claim(None) else {
            return;
        };
        drop(spawn_cleanup(
            self.state.clone(),
            claimed,
            "firmware execute request was cancelled",
            phase,
        ));
    }
}

fn spawn_cleanup(
    state: AppState,
    claimed: crate::sessions::ClaimedFirmwareCommand,
    reason: &'static str,
    phase: CancellationPhase,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        match phase {
            CancellationPhase::PrePublish => {
                super::finish_pre_publish_failure(&state, claimed, reason).await;
            }
            CancellationPhase::OutcomeUnknown => {
                super::finish_cancelled_commands(&state, vec![claimed], reason).await;
            }
        }
    })
}

pub(super) async fn wait_for_cleanup(cleanup: Option<JoinHandle<()>>) {
    if let Some(cleanup) = cleanup {
        cleanup
            .await
            .expect("firmware execute cleanup task must complete");
    }
}
