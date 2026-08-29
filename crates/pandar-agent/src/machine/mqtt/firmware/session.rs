//! A fresh firmware session excludes reports seen before its publish barrier and reports seen before
//! its own `Outgoing::Publish`. MQTT cannot disambiguate a delayed older acknowledgement that arrives
//! afterwards with both the same command and a reused sequence id.

#[cfg(test)]
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64},
};

use tokio::sync::{mpsc, oneshot};

use super::FirmwareMqttCommand;
use crate::machine::FirmwarePublishTransition;
use crate::machine::mqtt::MachineReport;

mod attempt;
mod connect;
#[cfg(test)]
mod connect_tests;
#[cfg(test)]
mod drop_pause;
mod owner;
mod pump;
#[cfg(test)]
mod pump_tests;
mod shutdown;
#[cfg(test)]
pub(crate) use connect::firmware_mqtt_options;
use owner::PumpOwner;
pub(crate) use owner::{FirmwareMqttTaskSet, FirmwarePumpAbortHandle};
#[cfg(test)]
use pump::ShutdownCompletionMode;
use pump::{
    AttemptEvent, FirmwareMqttAttemptFailure, FirmwareMqttOperationPhase, PumpRequest,
    attempt_failure,
};
#[cfg(test)]
use std::sync::atomic::Ordering;

pub(crate) struct FirmwareMqttSession {
    requests: mpsc::Sender<PumpRequest>,
    pump: Option<PumpOwner>,
    abort: FirmwarePumpAbortHandle,
    #[cfg(test)]
    received_ordinal: Arc<AtomicU64>,
    #[cfg(test)]
    pump_finished: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(test)]
    pump_reaped: Arc<AtomicBool>,
    #[cfg(test)]
    shutdown_pause: Option<FirmwareBarrierPause>,
    #[cfg(test)]
    shutdown_completion_mode: ShutdownCompletionMode,
}

pub(crate) struct FirmwareMqttAttempt {
    events: mpsc::UnboundedReceiver<AttemptEvent>,
    published: bool,
}

#[derive(Debug)]
pub(crate) struct FirmwareMqttReport {
    #[cfg(test)]
    pub(crate) ordinal: u64,
    pub(crate) payload: MachineReport,
}

pub(crate) struct FirmwareBarrierPause {
    reached: oneshot::Sender<()>,
    release: oneshot::Receiver<()>,
}

#[cfg(test)]
pub(crate) struct FirmwareBarrierPauseHandle {
    reached: oneshot::Receiver<()>,
    release: oneshot::Sender<()>,
}

#[cfg(test)]
pub(crate) struct FirmwarePumpDropPause {
    state: Arc<FirmwarePumpDropPauseState>,
}

#[cfg(test)]
pub(crate) struct FirmwarePumpDropPauseHandle {
    state: Arc<FirmwarePumpDropPauseState>,
}

#[cfg(test)]
struct FirmwarePumpDropPauseState {
    reached: AtomicBool,
    release: AtomicBool,
}

#[cfg(test)]
pub(crate) fn firmware_barrier_pause() -> (FirmwareBarrierPause, FirmwareBarrierPauseHandle) {
    let (reached_sender, reached) = oneshot::channel();
    let (release, release_receiver) = oneshot::channel();
    (
        FirmwareBarrierPause {
            reached: reached_sender,
            release: release_receiver,
        },
        FirmwareBarrierPauseHandle { reached, release },
    )
}

#[cfg(test)]
pub(crate) fn firmware_pump_drop_pause() -> (FirmwarePumpDropPause, FirmwarePumpDropPauseHandle) {
    let state = Arc::new(FirmwarePumpDropPauseState {
        reached: AtomicBool::new(false),
        release: AtomicBool::new(false),
    });
    (
        FirmwarePumpDropPause {
            state: Arc::clone(&state),
        },
        FirmwarePumpDropPauseHandle { state },
    )
}

impl FirmwareMqttSession {
    pub(crate) fn pump_abort_handle(&self) -> FirmwarePumpAbortHandle {
        self.abort.clone()
    }

    pub(crate) async fn publish(
        &self,
        command: FirmwareMqttCommand,
    ) -> anyhow::Result<FirmwareMqttAttempt> {
        let (events, receiver) = mpsc::unbounded_channel();
        self.requests
            .send(PumpRequest::Publish {
                command,
                events,
                transition: None,
            })
            .await
            .map_err(|_| {
                attempt_failure(
                    false,
                    FirmwareMqttOperationPhase::Send,
                    anyhow::anyhow!("firmware MQTT pump request channel closed")
                        .context("send firmware publish request to MQTT pump"),
                )
            })?;
        Ok(FirmwareMqttAttempt {
            events: receiver,
            published: false,
        })
    }

    pub(crate) fn publish_with_transition(
        &self,
        command: FirmwareMqttCommand,
        transition: FirmwarePublishTransition,
    ) -> anyhow::Result<FirmwareMqttAttempt> {
        let (events, receiver) = mpsc::unbounded_channel();
        match self.requests.try_send(PumpRequest::Publish {
            command,
            events,
            transition: Some(Box::new(transition)),
        }) {
            Ok(()) => Ok(FirmwareMqttAttempt {
                events: receiver,
                published: false,
            }),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(attempt_failure(
                false,
                FirmwareMqttOperationPhase::Send,
                anyhow::anyhow!("firmware MQTT pump request channel closed")
                    .context("send firmware publish request to ended MQTT pump"),
            )),
            Err(mpsc::error::TrySendError::Full(_)) => {
                unreachable!("fresh firmware MQTT session accepts exactly one publish request")
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn received_ordinal_for_test(&self) -> u64 {
        self.received_ordinal.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn pump_finished_for_test(&self) -> bool {
        self.pump_finished.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn pump_finished_flag_for_test(&self) -> Arc<std::sync::atomic::AtomicBool> {
        Arc::clone(&self.pump_finished)
    }

    #[cfg(test)]
    pub(crate) fn pump_reaped_flag_for_test(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.pump_reaped)
    }

    #[cfg(test)]
    pub(crate) fn pump_abort_requested_flag_for_test(&self) -> Arc<AtomicBool> {
        self.abort.requested_flag()
    }
}

#[cfg(test)]
impl FirmwareBarrierPauseHandle {
    pub(crate) async fn wait_until_reached(&mut self) {
        (&mut self.reached)
            .await
            .expect("firmware pump ended before publish barrier pause");
    }

    pub(crate) fn release(self) {
        let _ = self.release.send(());
    }

    pub(crate) fn cancel(self) {
        drop(self.release);
    }
}

#[cfg(test)]
impl FirmwarePumpDropPause {
    fn block_until_released(&self) {
        self.state.reached.store(true, Ordering::SeqCst);
        while !self.state.release.load(Ordering::SeqCst) {
            std::thread::yield_now();
        }
    }
}

#[cfg(test)]
impl FirmwarePumpDropPauseHandle {
    pub(crate) async fn wait_until_reached(&self) {
        while !self.state.reached.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    }

    pub(crate) fn release(self) {
        self.state.release.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
impl Drop for FirmwarePumpDropPauseHandle {
    fn drop(&mut self) {
        self.state.release.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
pub(crate) fn is_firmware_pre_publish_failure(error: &anyhow::Error) -> bool {
    firmware_mqtt_failure_phase(error) == Some(false)
}

#[cfg(test)]
pub(crate) fn is_firmware_post_publish_failure(error: &anyhow::Error) -> bool {
    firmware_mqtt_failure_phase(error) == Some(true)
}

pub(crate) fn firmware_mqtt_failure_phase(error: &anyhow::Error) -> Option<bool> {
    error
        .downcast_ref::<FirmwareMqttAttemptFailure>()
        .map(|failure| failure.after_publish)
}

pub(crate) fn firmware_mqtt_failure(after_publish: bool, message: String) -> anyhow::Error {
    attempt_failure(
        after_publish,
        FirmwareMqttOperationPhase::Session,
        anyhow::Error::msg(message),
    )
}
