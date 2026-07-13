use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::{Notify, mpsc};

use crate::machine::{
    FirmwareControlOutcome, FirmwareControlPhase, FirmwareExecuteRequest, FirmwareMachineGateway,
    FirmwareModulesDelivery, FirmwarePrepareRequest, FirmwarePreparedObservation,
    FirmwareRefreshRequest,
    file_transfer::MachineFileTransfer,
    mqtt::{
        BambuMqttTransport, FirmwareMqttSession, FirmwareMqttTaskSet, firmware_command_payload,
    },
};

use super::super::firmware_gateway::complete_firmware_control_with_transition_for_test;
use super::TestRuntimeBambuMachineGateway;

#[derive(Clone)]
pub(crate) struct FirmwareExecutePause {
    state: Arc<FirmwareExecutePauseState>,
}

pub(super) struct FirmwareExecutePauseState {
    pub(super) blocked: Notify,
    pub(super) release: Notify,
    pub(super) cancelled: AtomicBool,
}

struct FirmwareSessionExecuteHook {
    session: FirmwareMqttSession,
    started: Arc<Notify>,
    publish: bool,
    abort_before_transition_release: Option<Arc<AtomicBool>>,
}

static FIRMWARE_SESSION_EXECUTE_HOOK: std::sync::OnceLock<
    tokio::sync::Mutex<HashMap<usize, FirmwareSessionExecuteHook>>,
> = std::sync::OnceLock::new();
static FIRMWARE_SESSION_TASK_SET: std::sync::OnceLock<
    tokio::sync::Mutex<HashMap<usize, FirmwareMqttTaskSet>>,
> = std::sync::OnceLock::new();

pub(crate) struct FirmwareSessionExecuteHandle {
    started: Arc<Notify>,
}

impl FirmwareExecutePause {
    pub(crate) async fn wait_until_blocked(&self) {
        self.state.blocked.notified().await;
    }

    pub(crate) fn release(&self) {
        self.state.release.notify_one();
    }

    pub(crate) fn was_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::SeqCst)
    }
}

impl FirmwareSessionExecuteHandle {
    pub(crate) async fn wait_until_started(&self) {
        self.started.notified().await;
    }
}

impl<T, F> TestRuntimeBambuMachineGateway<T, F> {
    pub(crate) async fn pause_firmware_execute(&self) -> FirmwareExecutePause {
        let state = Arc::new(FirmwareExecutePauseState {
            blocked: Notify::new(),
            release: Notify::new(),
            cancelled: AtomicBool::new(false),
        });
        *self.firmware_execute_pause.lock().await = Some(Arc::clone(&state));
        FirmwareExecutePause { state }
    }

    pub(crate) fn firmware_publish_count(&self) -> usize {
        self.firmware_publish_count.load(Ordering::SeqCst)
    }

    pub(crate) async fn install_firmware_session_for_execute(
        &self,
        session: FirmwareMqttSession,
        task_set: FirmwareMqttTaskSet,
    ) -> FirmwareSessionExecuteHandle {
        let started = Arc::new(Notify::new());
        let hook = FirmwareSessionExecuteHook {
            session,
            started: Arc::clone(&started),
            publish: false,
            abort_before_transition_release: None,
        };
        let gateway_id = std::ptr::from_ref(self).addr();
        let previous = FIRMWARE_SESSION_EXECUTE_HOOK
            .get_or_init(Default::default)
            .lock()
            .await
            .insert(gateway_id, hook);
        assert!(
            previous.is_none(),
            "firmware session execute hook is unique"
        );
        let previous = FIRMWARE_SESSION_TASK_SET
            .get_or_init(Default::default)
            .lock()
            .await
            .insert(gateway_id, task_set);
        assert!(previous.is_none(), "firmware session task set is unique");
        FirmwareSessionExecuteHandle { started }
    }

    pub(crate) async fn install_firmware_publish_session_for_execute(
        &self,
        session: FirmwareMqttSession,
        task_set: FirmwareMqttTaskSet,
        abort_before_transition_release: Arc<AtomicBool>,
    ) -> FirmwareSessionExecuteHandle {
        let started = Arc::new(Notify::new());
        let hook = FirmwareSessionExecuteHook {
            session,
            started: Arc::clone(&started),
            publish: true,
            abort_before_transition_release: Some(abort_before_transition_release),
        };
        let gateway_id = std::ptr::from_ref(self).addr();
        let previous = FIRMWARE_SESSION_EXECUTE_HOOK
            .get_or_init(Default::default)
            .lock()
            .await
            .insert(gateway_id, hook);
        assert!(
            previous.is_none(),
            "firmware session execute hook is unique"
        );
        let previous = FIRMWARE_SESSION_TASK_SET
            .get_or_init(Default::default)
            .lock()
            .await
            .insert(gateway_id, task_set);
        assert!(previous.is_none(), "firmware session task set is unique");
        FirmwareSessionExecuteHandle { started }
    }
}

#[async_trait]
impl<T, F> FirmwareMachineGateway for TestRuntimeBambuMachineGateway<T, F>
where
    T: BambuMqttTransport + Clone + Send + Sync + 'static,
    F: MachineFileTransfer + Clone + Send + Sync + 'static,
{
    async fn refresh_firmware_version(
        &self,
        _request: FirmwareRefreshRequest,
    ) -> anyhow::Result<FirmwareModulesDelivery> {
        anyhow::bail!("test runtime firmware refresh is not configured")
    }

    async fn prepare_firmware_control(
        &self,
        request: FirmwarePrepareRequest,
    ) -> anyhow::Result<FirmwarePreparedObservation> {
        self.firmware.prepare_firmware_control(request).await
    }

    async fn execute_firmware_control(
        &self,
        request: FirmwareExecuteRequest,
        phases: mpsc::UnboundedSender<FirmwareControlPhase>,
    ) -> anyhow::Result<FirmwareControlOutcome> {
        let execution = self.firmware.claim_firmware_execute(&request).await?;
        let session_hook = FIRMWARE_SESSION_EXECUTE_HOOK
            .get_or_init(Default::default)
            .lock()
            .await
            .remove(&std::ptr::from_ref(self).addr());
        if let Some(mut hook) = session_hook {
            hook.started.notify_one();
            if hook.publish {
                let mut transition = execution.publish_transition().await?;
                let abort_requested = hook.session.pump_abort_requested_flag_for_test();
                let abort_before_transition_release = hook
                    .abort_before_transition_release
                    .take()
                    .expect("publish hook observes transition release");
                transition.observe_release_for_test(move || {
                    abort_before_transition_release
                        .store(abort_requested.load(Ordering::SeqCst), Ordering::SeqCst);
                });
                return complete_firmware_control_with_transition_for_test(
                    &mut hook.session,
                    firmware_command_payload(&request.command),
                    phases,
                    transition,
                )
                .await;
            }
            let _session = hook.session;
            std::future::pending::<()>().await;
        }
        if let Some(state) = self.firmware_execute_pause.lock().await.take() {
            let mut guard = FirmwareExecutePauseGuard {
                state: Arc::clone(&state),
                completed: false,
            };
            state.blocked.notify_one();
            state.release.notified().await;
            guard.completed = true;
            let transition = execution.publish_transition().await?;
            drop(transition);
            self.firmware_publish_count.fetch_add(1, Ordering::SeqCst);
            let _ = phases.send(FirmwareControlPhase::Published);
        }
        anyhow::bail!("test runtime firmware execute is not configured")
    }

    async fn cancel_firmware_session(&self, session_epoch: u64) -> anyhow::Result<()> {
        let teardown = match FIRMWARE_SESSION_TASK_SET
            .get_or_init(Default::default)
            .lock()
            .await
            .remove(&std::ptr::from_ref(self).addr())
        {
            Some(task_set) => task_set.abort_and_join_all().await,
            None => Ok(()),
        };
        self.firmware.cancel_firmware_session(session_epoch).await;
        teardown
    }
}

struct FirmwareExecutePauseGuard {
    state: Arc<FirmwareExecutePauseState>,
    completed: bool,
}

impl Drop for FirmwareExecutePauseGuard {
    fn drop(&mut self) {
        if !self.completed {
            self.state.cancelled.store(true, Ordering::SeqCst);
        }
    }
}
