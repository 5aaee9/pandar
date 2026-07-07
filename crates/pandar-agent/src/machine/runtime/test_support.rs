use std::{collections::VecDeque, sync::Arc};

use anyhow::bail;
use tokio::sync::Notify;

use super::*;
use crate::machine::{
    diagnostics::PrinterDiagnosticResult,
    discovery::{DiscoveredPrinter, PrinterDiscoveryResult},
    file_transfer::{MachineFileTransfer, TransferModeCache},
    mqtt::BambuMqttTransport,
};

pub(crate) struct TestRuntimeBambuMachineGateway<T, F> {
    inner: tokio::sync::Mutex<ConfiguredBambuMachineGateway<T, F>>,
    discovered_printers: tokio::sync::Mutex<Vec<DiscoveredPrinter>>,
    report_tasks: tokio::sync::Mutex<HashMap<String, JoinHandle<()>>>,
    command_transports: tokio::sync::Mutex<VecDeque<anyhow::Result<T>>>,
    report_preparation_errors: tokio::sync::Mutex<VecDeque<anyhow::Error>>,
    report_task_replacement_pause: tokio::sync::Mutex<Option<ReportTaskReplacementPause>>,
    redaction_access_codes: StdMutex<Vec<String>>,
    transfer: F,
    report_timeout: Duration,
}

impl<T, F> TestRuntimeBambuMachineGateway<T, F>
where
    T: BambuMqttTransport + Send + Sync,
    F: MachineFileTransfer + Clone + Send + Sync,
{
    pub(crate) fn new(
        printers: Vec<(BambuPrinterEndpoint, T, F)>,
        transfer: F,
        report_timeout: Duration,
    ) -> Self {
        let redaction_access_codes = printers
            .iter()
            .map(|(endpoint, _, _)| endpoint.access_code.clone())
            .collect();
        let discovered_printers = printers
            .iter()
            .map(|(endpoint, _, _)| DiscoveredPrinter {
                serial_number: Some(endpoint.serial.clone()),
                host: endpoint.host.clone(),
                name: endpoint.name.clone(),
                model: endpoint.model.clone(),
                source: "ssdp",
            })
            .collect();
        Self {
            inner: tokio::sync::Mutex::new(ConfiguredBambuMachineGateway::with_file_transfer(
                printers,
                report_timeout,
                TransferModeCache::default(),
            )),
            discovered_printers: tokio::sync::Mutex::new(discovered_printers),
            report_tasks: tokio::sync::Mutex::new(HashMap::new()),
            command_transports: tokio::sync::Mutex::new(VecDeque::new()),
            report_preparation_errors: tokio::sync::Mutex::new(VecDeque::new()),
            report_task_replacement_pause: tokio::sync::Mutex::new(None),
            redaction_access_codes: StdMutex::new(redaction_access_codes),
            transfer,
            report_timeout,
        }
    }

    pub(crate) async fn set_discovered_printers(&self, printers: Vec<DiscoveredPrinter>) {
        *self.discovered_printers.lock().await = printers;
    }

    pub(crate) async fn push_command_transport(&self, transport: T) {
        self.command_transports
            .lock()
            .await
            .push_back(Ok(transport));
    }

    pub(crate) async fn push_report_preparation_error(&self, error: anyhow::Error) {
        self.report_preparation_errors.lock().await.push_back(error);
    }

    pub(crate) async fn pause_report_task_replacement(&self) -> ReportTaskReplacementPause {
        let pause = ReportTaskReplacementPause::new();
        *self.report_task_replacement_pause.lock().await = Some(pause.clone());
        pause
    }

    pub(crate) async fn report_task_count(&self, serial: &str) -> usize {
        if self.report_tasks.lock().await.contains_key(serial) {
            1
        } else {
            0
        }
    }

    async fn next_command_transport(&self) -> anyhow::Result<T> {
        self.command_transports
            .lock()
            .await
            .pop_front()
            .unwrap_or_else(|| Err(anyhow::anyhow!("missing test command transport")))
    }

    async fn prepare_report_forwarding(&self) -> anyhow::Result<()> {
        if let Some(error) = self.report_preparation_errors.lock().await.pop_front() {
            return Err(error);
        }
        Ok(())
    }

    async fn pause_before_report_task_replacement(&self) {
        let pause = self.report_task_replacement_pause.lock().await.take();
        if let Some(pause) = pause {
            pause.wait_for_release().await;
        }
    }

    fn record_access_code(&self, endpoint: &BambuPrinterEndpoint) {
        let mut access_codes = self.redaction_access_codes.lock().unwrap();
        access_codes.retain(|access_code| access_code != &endpoint.access_code);
        access_codes.push(endpoint.access_code.clone());
    }
}

#[async_trait]
impl<T, F> BambuMachineGateway for TestRuntimeBambuMachineGateway<T, F>
where
    T: BambuMqttTransport + Send + Sync,
    F: MachineFileTransfer + Clone + Send + Sync,
{
    fn redact_error(&self, message: &str) -> String {
        redact_known_access_codes(message, self.redaction_access_codes.lock().unwrap().clone())
    }

    async fn discover_printers(
        &self,
        _timeout_seconds: u32,
    ) -> anyhow::Result<PrinterDiscoveryResult> {
        Ok(PrinterDiscoveryResult::new(
            self.discovered_printers.lock().await.clone(),
        ))
    }

    async fn diagnose_printer(
        &self,
        serial_number: &str,
    ) -> anyhow::Result<PrinterDiagnosticResult> {
        self.inner
            .lock()
            .await
            .diagnose_printer(serial_number)
            .await
    }

    async fn refresh_printers(&self) -> anyhow::Result<Vec<PrinterRefreshResult>> {
        self.inner.lock().await.refresh_printers().await
    }

    async fn refresh_printer_materials(
        &self,
        serial_number: &str,
        printer_id: Option<&str>,
    ) -> anyhow::Result<MaterialRefreshResult> {
        self.inner
            .lock()
            .await
            .refresh_printer_materials(serial_number, printer_id)
            .await
    }

    async fn validate_printer(&self, serial_number: &str) -> anyhow::Result<()> {
        self.inner
            .lock()
            .await
            .validate_printer(serial_number)
            .await
    }

    async fn print_project_file(
        &self,
        serial_number: &str,
        command: &PrintProjectFile,
        artifact: Vec<u8>,
    ) -> anyhow::Result<PrintProjectDispatchResult> {
        self.inner
            .lock()
            .await
            .print_project_file(serial_number, command, artifact)
            .await
    }

    async fn operate_printer(
        &self,
        serial_number: &str,
        operation: PrinterOperation,
    ) -> anyhow::Result<PrinterOperationDispatchResult> {
        self.inner
            .lock()
            .await
            .operate_printer(serial_number, operation)
            .await
    }

    async fn link_printer(
        &self,
        endpoint: BambuPrinterEndpoint,
        _config: &AgentConfig,
        _sender: &mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<MachineSnapshot> {
        let command_transport = self.next_command_transport().await?;
        let mut inner = self.inner.lock().await;
        let snapshot = refresh_printer(&command_transport, &endpoint, self.report_timeout)
            .await
            .with_context(|| format!("validate runtime printer {}", endpoint.serial))?
            .snapshot;
        self.prepare_report_forwarding().await?;
        inner.replace_printer(endpoint.clone(), command_transport, self.transfer.clone());
        self.pause_before_report_task_replacement().await;
        if let Some(task) = self.report_tasks.lock().await.remove(&endpoint.serial) {
            task.abort();
        }
        self.report_tasks.lock().await.insert(
            endpoint.serial.clone(),
            tokio::spawn(async { std::future::pending::<()>().await }),
        );
        self.record_access_code(&endpoint);
        Ok(snapshot)
    }
}

#[derive(Clone)]
pub(crate) struct ReportTaskReplacementPause {
    state: Arc<ReportTaskReplacementPauseState>,
}

struct ReportTaskReplacementPauseState {
    blocked: Notify,
    release: Notify,
}

impl ReportTaskReplacementPause {
    fn new() -> Self {
        Self {
            state: Arc::new(ReportTaskReplacementPauseState {
                blocked: Notify::new(),
                release: Notify::new(),
            }),
        }
    }

    pub(crate) async fn wait_until_blocked(&self) {
        self.state.blocked.notified().await;
    }

    pub(crate) fn release(&self) {
        self.state.release.notify_waiters();
    }

    async fn wait_for_release(&self) {
        self.state.blocked.notify_waiters();
        self.state.release.notified().await;
    }
}

pub(crate) async fn assert_locked_for_a_moment<T, F>(
    gateway: &TestRuntimeBambuMachineGateway<T, F>,
) -> anyhow::Result<()>
where
    T: BambuMqttTransport + Send + Sync,
    F: MachineFileTransfer + Clone + Send + Sync,
{
    if gateway.inner.try_lock().is_ok() {
        bail!("runtime gateway lock was available while link_printer validation was paused");
    }
    Ok(())
}
