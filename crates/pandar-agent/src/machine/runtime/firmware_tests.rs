use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use tokio::sync::{Notify, mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;

use super::firmware::{pause_refresh_child_drop_for_test, refresh_runtime_printers_with_firmware};
use crate::{
    AgentConfig,
    backoff::RunOutcome,
    command_stream::run_command_stream_until_cancelled,
    machine::{
        BambuMachineGateway, BambuPrinterEndpoint, ConfiguredBambuMachineGateway,
        DeviceFeatureCache, FirmwareControlOutcome, FirmwareControlPhase, FirmwareExecuteRequest,
        FirmwareMachineGateway, FirmwareModulesDelivery, FirmwareObservationCache,
        FirmwarePrepareRequest, FirmwarePreparedObservation, FirmwareRefreshRequest,
        MaterialRefreshResult, PrintProjectDispatchResult, PrinterRefreshResult,
        diagnostics::PrinterDiagnosticResult,
        discovery::PrinterDiscoveryResult,
        mqtt::{BambuMqttTransport, PublishedMqttCommand},
    },
};
use pandar_protocol::agent::v1::{
    AgentEvent, HubCommand, PrintProjectFile, RefreshPrinters, hub_command,
};

const SESSION_EPOCH: u64 = 901;

#[derive(Clone)]
struct BlockingRefreshTransport {
    started: Arc<Notify>,
}

impl BlockingRefreshTransport {
    fn new() -> Self {
        Self {
            started: Arc::new(Notify::new()),
        }
    }

    async fn wait_until_started(&self) {
        self.started.notified().await;
    }
}

#[async_trait]
impl BambuMqttTransport for BlockingRefreshTransport {
    async fn subscribe(&self, _topic: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn publish(&self, _command: PublishedMqttCommand) -> anyhow::Result<()> {
        Ok(())
    }

    async fn next_report(&self, _timeout: Duration) -> anyhow::Result<serde_json::Value> {
        self.started.notify_one();
        std::future::pending().await
    }
}

struct RefreshLifecycleGateway {
    inner: Arc<tokio::sync::Mutex<ConfiguredBambuMachineGateway<BlockingRefreshTransport>>>,
    firmware: FirmwareObservationCache,
    device_features: DeviceFeatureCache,
    current_sender: tokio::sync::Mutex<Option<mpsc::Sender<AgentEvent>>>,
    config: AgentConfig,
}

impl RefreshLifecycleGateway {
    fn new(config: AgentConfig, transport: BlockingRefreshTransport, serial: &str) -> Self {
        Self {
            inner: Arc::new(tokio::sync::Mutex::new(ConfiguredBambuMachineGateway::new(
                vec![(
                    BambuPrinterEndpoint {
                        host: "192.0.2.10".into(),
                        serial: serial.into(),
                        access_code: "secret".into(),
                        model: Some("X1".into()),
                        name: Some("office".into()),
                    },
                    transport,
                )],
                Duration::from_secs(60),
            ))),
            firmware: FirmwareObservationCache::default(),
            device_features: DeviceFeatureCache::default(),
            current_sender: tokio::sync::Mutex::new(None),
            config,
        }
    }

    async fn prepare_session(&self, sender: &mpsc::Sender<AgentEvent>) {
        *self.current_sender.lock().await = Some(sender.clone());
    }

    async fn clear_session_sender(&self, sender: &mpsc::Sender<AgentEvent>) {
        let mut current = self.current_sender.lock().await;
        if current
            .as_ref()
            .is_some_and(|current| current.same_channel(sender))
        {
            *current = None;
        }
    }

    async fn has_current_sender(&self) -> bool {
        self.current_sender.lock().await.is_some()
    }
}

#[async_trait]
impl BambuMachineGateway for RefreshLifecycleGateway {
    fn redact_error(&self, message: &str) -> String {
        message.to_owned()
    }

    async fn discover_printers(
        &self,
        _timeout_seconds: u32,
    ) -> anyhow::Result<PrinterDiscoveryResult> {
        unreachable!()
    }

    async fn diagnose_printer(
        &self,
        _serial_number: &str,
    ) -> anyhow::Result<PrinterDiagnosticResult> {
        unreachable!()
    }

    async fn refresh_printers(&self) -> anyhow::Result<Vec<PrinterRefreshResult>> {
        let event_context = self
            .current_sender
            .lock()
            .await
            .clone()
            .map(|sender| (self.config.clone(), sender));
        refresh_runtime_printers_with_firmware(
            Arc::clone(&self.inner),
            self.firmware.clone(),
            self.device_features.clone(),
            event_context,
            Duration::from_secs(60),
        )
        .await
    }

    async fn refresh_printer_materials(
        &self,
        _serial_number: &str,
        _printer_id: Option<&str>,
    ) -> anyhow::Result<MaterialRefreshResult> {
        unreachable!()
    }

    async fn validate_printer(&self, _serial_number: &str) -> anyhow::Result<()> {
        unreachable!()
    }

    async fn print_project_file(
        &self,
        _serial_number: &str,
        _command: &PrintProjectFile,
        _artifact: Vec<u8>,
    ) -> anyhow::Result<PrintProjectDispatchResult> {
        unreachable!()
    }
}

#[async_trait]
impl FirmwareMachineGateway for RefreshLifecycleGateway {
    async fn refresh_firmware_version(
        &self,
        _request: FirmwareRefreshRequest,
    ) -> anyhow::Result<FirmwareModulesDelivery> {
        unreachable!()
    }

    async fn prepare_firmware_control(
        &self,
        _request: FirmwarePrepareRequest,
    ) -> anyhow::Result<FirmwarePreparedObservation> {
        unreachable!()
    }

    async fn execute_firmware_control(
        &self,
        _request: FirmwareExecuteRequest,
        _phases: mpsc::UnboundedSender<FirmwareControlPhase>,
    ) -> anyhow::Result<FirmwareControlOutcome> {
        unreachable!()
    }

    async fn cancel_firmware_session(&self, session_epoch: u64) -> anyhow::Result<()> {
        self.firmware.cancel_firmware_session(session_epoch).await;
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Stop {
    Eof,
    Status,
    Cancellation,
}

struct CleanupStarted {
    started: AtomicBool,
    notify: Notify,
}

impl CleanupStarted {
    fn new() -> Self {
        Self {
            started: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    fn mark(&self) {
        self.started.store(true, Ordering::SeqCst);
        self.notify.notify_one();
    }

    async fn wait(&self) {
        while !self.started.load(Ordering::SeqCst) {
            self.notify.notified().await;
        }
    }
}

async fn assert_refresh_child_drops_before_session_cleanup(stop: Stop, serial: &'static str) {
    let config = AgentConfig {
        hub_grpc_url: "http://127.0.0.1:50051".into(),
        hub_api_url: None,
        agent_name: "test-agent".into(),
        agent_id: "agent-1".into(),
        tenant_id: "tenant-1".into(),
        agent_credential: "credential".into(),
        agent_version: "test".into(),
        printers: "[]".into(),
    };
    let transport = BlockingRefreshTransport::new();
    let gateway = Arc::new(RefreshLifecycleGateway::new(
        config.clone(),
        transport.clone(),
        serial,
    ));
    let drop_pause = pause_refresh_child_drop_for_test(serial);
    let cleanup_started = Arc::new(CleanupStarted::new());
    let (events, mut event_receiver) = mpsc::channel(16);
    gateway.prepare_session(&events).await;
    let (commands, command_receiver) = mpsc::channel(2);
    let (cancel, cancelled) = oneshot::channel::<()>();
    let session = tokio::spawn({
        let gateway = Arc::clone(&gateway);
        let cleanup_started = Arc::clone(&cleanup_started);
        async move {
            let outcome = run_command_stream_until_cancelled(
                &config,
                Arc::clone(&gateway),
                &events,
                ReceiverStream::new(command_receiver),
                SESSION_EPOCH,
                async move {
                    let _ = cancelled.await;
                },
            )
            .await;
            cleanup_started.mark();
            gateway
                .cancel_firmware_session(SESSION_EPOCH)
                .await
                .unwrap();
            gateway.clear_session_sender(&events).await;
            outcome
        }
    });
    commands
        .send(Ok(HubCommand {
            command_id: format!("refresh-{serial}"),
            command: Some(hub_command::Command::RefreshPrinters(RefreshPrinters {})),
        }))
        .await
        .unwrap();
    transport.wait_until_started().await;

    match stop {
        Stop::Eof => drop(commands),
        Stop::Status => {
            commands
                .send(Err(Status::unavailable(
                    "refresh lifecycle status sentinel",
                )))
                .await
                .unwrap();
            drop(commands);
        }
        Stop::Cancellation => {
            cancel.send(()).unwrap();
            drop(commands);
        }
    }

    tokio::time::timeout(Duration::from_secs(1), drop_pause.wait_until_started())
        .await
        .expect("refresh child Drop must reach the deterministic join pause");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), cleanup_started.wait())
            .await
            .is_err(),
        "session cleanup must wait for refresh child Drop"
    );
    assert_eq!(gateway.firmware.ended_session_epoch_for_test(), 0);
    assert!(gateway.has_current_sender().await);
    assert!(!drop_pause.was_dropped());

    drop_pause.release();
    let outcome = tokio::time::timeout(Duration::from_secs(1), session)
        .await
        .expect("session cleanup must finish after refresh child Drop")
        .unwrap();
    match stop {
        Stop::Status => assert!(
            format!("{:#}", outcome.unwrap_err()).contains("refresh lifecycle status sentinel")
        ),
        Stop::Eof | Stop::Cancellation => {
            assert_eq!(outcome.unwrap(), RunOutcome::ConnectedThenEnded)
        }
    }
    assert!(drop_pause.was_dropped());
    assert_eq!(
        gateway.firmware.ended_session_epoch_for_test(),
        SESSION_EPOCH
    );
    assert!(!gateway.has_current_sender().await);
    tokio::time::timeout(Duration::from_secs(1), async {
        while event_receiver.recv().await.is_some() {}
    })
    .await
    .expect("the exact session sender must clear after refresh child Drop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn firmware_refresh_command_eof_drops_child_before_epoch_cancel_and_sender_clear() {
    assert_refresh_child_drops_before_session_cleanup(Stop::Eof, "REFRESH-EOF").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn firmware_refresh_stream_status_drops_child_before_epoch_cancel_and_sender_clear() {
    assert_refresh_child_drops_before_session_cleanup(Stop::Status, "REFRESH-STATUS").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn firmware_refresh_normal_worker_cancellation_drops_child_before_session_cleanup() {
    assert_refresh_child_drops_before_session_cleanup(Stop::Cancellation, "REFRESH-CANCEL").await;
}
