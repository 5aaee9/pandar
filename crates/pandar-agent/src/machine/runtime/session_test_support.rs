use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use anyhow::bail;
use tokio::sync::{Notify, mpsc, oneshot};

use super::{LinkValidationResult, RuntimeBambuMachineGateway};
use crate::machine::{
    BambuPrinterEndpoint, FirmwareReportContext, mqtt::RumqttcBambuMqttTransport,
};
use pandar_protocol::agent::v1::AgentEvent;

pub(super) struct PartialPrepareReportHookState {
    registered: Notify,
    fail_receiver: tokio::sync::Mutex<Option<oneshot::Receiver<()>>>,
    forwarder: Arc<ReportForwarderState>,
}

pub(crate) struct PartialPrepareReportHandle {
    state: Arc<PartialPrepareReportHookState>,
    fail: Option<oneshot::Sender<()>>,
}

pub(crate) struct ReportForwarderHandle {
    state: Arc<ReportForwarderState>,
}

pub(super) struct ReportJoinPauseState {
    serial: String,
    reached: Notify,
    release: Notify,
}

pub(crate) struct ReportJoinPauseHandle {
    state: Arc<ReportJoinPauseState>,
}

pub(super) struct HeartbeatPanicState {
    started: Notify,
    release: Notify,
    unwound: Notify,
}

pub(crate) struct HeartbeatPanicHandle {
    state: Arc<HeartbeatPanicState>,
}

struct HeartbeatPanicDropMarker(Arc<HeartbeatPanicState>);

struct ReportForwarderState {
    dropped: AtomicBool,
}

struct ReportForwarderDropMarker(Arc<ReportForwarderState>);

impl RuntimeBambuMachineGateway {
    pub(crate) async fn inject_link_validation_result_for_test(
        &self,
        result: LinkValidationResult,
    ) {
        *self.link_validation_result.lock().await = Some(result);
    }

    pub(crate) async fn panic_heartbeat_for_test(&self) -> HeartbeatPanicHandle {
        let state = Arc::new(HeartbeatPanicState {
            started: Notify::new(),
            release: Notify::new(),
            unwound: Notify::new(),
        });
        *self.heartbeat_panic_hook.lock().await = Some(Arc::clone(&state));
        HeartbeatPanicHandle { state }
    }

    pub(crate) async fn panic_heartbeat_for_test_if_installed(&self) {
        let Some(state) = self.heartbeat_panic_hook.lock().await.take() else {
            return;
        };
        let _marker = HeartbeatPanicDropMarker(Arc::clone(&state));
        state.started.notify_one();
        state.release.notified().await;
        panic!("firmware heartbeat panic sentinel");
    }

    pub(crate) async fn pause_report_join_for_test(&self, serial: &str) -> ReportJoinPauseHandle {
        let state = Arc::new(ReportJoinPauseState {
            serial: serial.into(),
            reached: Notify::new(),
            release: Notify::new(),
        });
        *self.report_join_pause.lock().await = Some(Arc::clone(&state));
        ReportJoinPauseHandle { state }
    }

    pub(super) async fn pause_report_join_for_test_if_installed(&self, serial: &str) {
        let state = {
            let mut pause = self.report_join_pause.lock().await;
            if pause.as_ref().is_some_and(|state| state.serial == serial) {
                pause.take()
            } else {
                None
            }
        };
        let Some(state) = state else {
            return;
        };
        state.reached.notify_one();
        state.release.notified().await;
    }

    pub(crate) async fn fail_prepare_after_first_report_forwarder_for_test(
        &self,
    ) -> PartialPrepareReportHandle {
        let (fail, fail_receiver) = oneshot::channel();
        let state = Arc::new(PartialPrepareReportHookState {
            registered: Notify::new(),
            fail_receiver: tokio::sync::Mutex::new(Some(fail_receiver)),
            forwarder: Arc::new(ReportForwarderState {
                dropped: AtomicBool::new(false),
            }),
        });
        *self.partial_prepare_report_hook.lock().await = Some(Arc::clone(&state));
        PartialPrepareReportHandle {
            state,
            fail: Some(fail),
        }
    }

    pub(super) async fn fail_partial_prepare_after_report_forwarder_for_test_if_installed(
        &self,
        endpoints: &[BambuPrinterEndpoint],
        sender: &mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<()> {
        let Some(state) = self.partial_prepare_report_hook.lock().await.take() else {
            return Ok(());
        };
        assert!(
            endpoints.len() >= 2,
            "partial prepare test needs two printers"
        );
        let marker = ReportForwarderDropMarker(Arc::clone(&state.forwarder));
        let report_sender = sender.clone();
        let task = tokio::spawn(async move {
            let _marker = marker;
            let _report_sender = report_sender;
            std::future::pending::<()>().await;
        });
        self.report_tasks
            .lock()
            .await
            .insert(endpoints[0].serial.clone(), task);
        state.registered.notify_one();
        let fail = state
            .fail_receiver
            .lock()
            .await
            .take()
            .expect("partial prepare failure receiver is installed");
        let _ = fail.await;
        bail!("partial prepare failure after first report forwarder")
    }

    pub(crate) async fn install_blocking_report_forwarder_for_test(
        &self,
        serial: &str,
    ) -> ReportForwarderHandle {
        let state = Arc::new(ReportForwarderState {
            dropped: AtomicBool::new(false),
        });
        let marker = ReportForwarderDropMarker(Arc::clone(&state));
        let report_sender = self
            .current_sender
            .lock()
            .await
            .clone()
            .expect("test report forwarder needs an active session sender");
        let task = tokio::spawn(async move {
            let _marker = marker;
            let _report_sender = report_sender;
            std::future::pending::<()>().await;
        });
        let previous = self.report_tasks.lock().await.insert(serial.into(), task);
        assert!(previous.is_none(), "test report forwarder serial is unique");
        ReportForwarderHandle { state }
    }

    pub(crate) async fn replace_report_forwarder_for_test(
        &self,
        serial: &str,
    ) -> anyhow::Result<()> {
        let endpoint = BambuPrinterEndpoint {
            host: "192.0.2.10".into(),
            serial: serial.into(),
            access_code: "test-access-code".into(),
            model: None,
            name: None,
        };
        let transport = RumqttcBambuMqttTransport::connect_for_reports(&endpoint);
        let sender = self
            .current_sender
            .lock()
            .await
            .clone()
            .expect("test report replacement needs an active session sender");
        self.replace_report_task_with_transport(
            endpoint,
            transport,
            &sender,
            FirmwareReportContext {
                cache: self.firmware.clone(),
                generation: 1,
            },
        )
        .await
    }

    pub(crate) async fn has_report_forwarder_for_test(&self, serial: &str) -> bool {
        self.report_tasks.lock().await.contains_key(serial)
    }

    pub(crate) async fn install_panicking_report_forwarder_for_test(&self, serial: &str) {
        let task = tokio::spawn(async move {
            panic!("firmware report forwarder panic sentinel");
        });
        while !task.is_finished() {
            tokio::task::yield_now().await;
        }
        let previous = self.report_tasks.lock().await.insert(serial.into(), task);
        assert!(previous.is_none(), "test report forwarder serial is unique");
    }
}

impl PartialPrepareReportHandle {
    pub(crate) async fn wait_until_registered(&mut self) {
        self.state.registered.notified().await;
    }

    pub(crate) fn fail(&mut self) {
        let _ = self
            .fail
            .take()
            .expect("partial prepare failure is sent once")
            .send(());
    }

    pub(crate) fn forwarder_was_dropped(&self) -> bool {
        self.state.forwarder.dropped.load(Ordering::SeqCst)
    }
}

impl ReportForwarderHandle {
    pub(crate) fn was_dropped(&self) -> bool {
        self.state.dropped.load(Ordering::SeqCst)
    }
}

impl ReportJoinPauseHandle {
    pub(crate) async fn wait_until_reached(&self) {
        self.state.reached.notified().await;
    }

    pub(crate) fn release(&self) {
        self.state.release.notify_one();
    }
}

impl HeartbeatPanicHandle {
    pub(crate) async fn wait_until_started(&self) {
        self.state.started.notified().await;
    }

    pub(crate) fn panic(&self) {
        self.state.release.notify_one();
    }

    pub(crate) async fn wait_until_unwound(&self) {
        self.state.unwound.notified().await;
    }
}

impl Drop for ReportForwarderDropMarker {
    fn drop(&mut self) {
        self.0.dropped.store(true, Ordering::SeqCst);
    }
}

impl Drop for HeartbeatPanicDropMarker {
    fn drop(&mut self) {
        self.0.unwound.notify_one();
    }
}
