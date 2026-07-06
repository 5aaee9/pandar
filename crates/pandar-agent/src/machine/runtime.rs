use std::{collections::HashMap, sync::Mutex as StdMutex, time::Duration};

use anyhow::Context;
use async_trait::async_trait;
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    AgentConfig,
    machine::{
        BambuMachineGateway, BambuPrinterEndpoint, ConfiguredBambuMachineGateway, MachineSnapshot,
        MaterialRefreshResult, PrinterOperation, PrinterOperationDispatchResult,
        PrinterRefreshResult,
        diagnostics::{redact_access_code, redact_known_access_codes},
        mqtt::{RumqttcBambuMqttTransport, forward_print_reports, refresh_printer},
        transfer::BambuMachineFileTransfer,
    },
    protocol::agent::v1::{AgentEvent, PrintProjectFile},
};

pub struct RuntimeBambuMachineGateway {
    inner: tokio::sync::Mutex<ConfiguredBambuMachineGateway<RumqttcBambuMqttTransport>>,
    report_tasks: tokio::sync::Mutex<HashMap<String, JoinHandle<()>>>,
    redaction_access_codes: StdMutex<Vec<String>>,
    config: AgentConfig,
    report_timeout: Duration,
}

impl RuntimeBambuMachineGateway {
    pub fn new(
        config: AgentConfig,
        printers: Vec<BambuPrinterEndpoint>,
        report_timeout: Duration,
    ) -> Self {
        let redaction_access_codes = printers
            .iter()
            .map(|endpoint| endpoint.access_code.clone())
            .collect();
        let inner = ConfiguredBambuMachineGateway::new(
            printers
                .into_iter()
                .map(|endpoint| {
                    let transport = RumqttcBambuMqttTransport::connect(&endpoint);
                    (endpoint, transport)
                })
                .collect(),
            report_timeout,
        );

        Self {
            inner: tokio::sync::Mutex::new(inner),
            report_tasks: tokio::sync::Mutex::new(HashMap::new()),
            redaction_access_codes: StdMutex::new(redaction_access_codes),
            config,
            report_timeout,
        }
    }

    pub async fn start_initial_report_forwarders(&self, sender: &mpsc::Sender<AgentEvent>) {
        let endpoints = self.inner.lock().await.endpoints();
        for endpoint in endpoints {
            self.replace_report_task(endpoint, sender).await;
        }
    }

    async fn replace_report_task(
        &self,
        endpoint: BambuPrinterEndpoint,
        sender: &mpsc::Sender<AgentEvent>,
    ) {
        let transport = RumqttcBambuMqttTransport::connect_for_reports(&endpoint);
        self.replace_report_task_with_transport(endpoint, transport, sender)
            .await;
    }

    async fn replace_report_task_with_transport(
        &self,
        endpoint: BambuPrinterEndpoint,
        transport: RumqttcBambuMqttTransport,
        sender: &mpsc::Sender<AgentEvent>,
    ) {
        let mut tasks = self.report_tasks.lock().await;
        if let Some(task) = tasks.remove(&endpoint.serial) {
            task.abort();
        }
        tasks.insert(
            endpoint.serial.clone(),
            self.spawn_report_task(endpoint, transport, sender.clone()),
        );
    }

    fn spawn_report_task(
        &self,
        endpoint: BambuPrinterEndpoint,
        transport: RumqttcBambuMqttTransport,
        sender: mpsc::Sender<AgentEvent>,
    ) -> JoinHandle<()> {
        let config = self.config.clone();
        let report_timeout = self.report_timeout;
        tokio::spawn(async move {
            if let Err(err) =
                forward_print_reports(&config, &transport, &endpoint, report_timeout, &sender).await
            {
                let error = redact_access_code(&format!("{err:#}"), &endpoint.access_code);
                tracing::warn!(
                    serial = %endpoint.serial,
                    error = %error,
                    "printer report forwarding ended"
                );
            }
        })
    }

    fn record_access_code(&self, endpoint: &BambuPrinterEndpoint) {
        let mut access_codes = self.redaction_access_codes.lock().unwrap();
        access_codes.retain(|access_code| access_code != &endpoint.access_code);
        access_codes.push(endpoint.access_code.clone());
    }
}

#[async_trait]
impl BambuMachineGateway for RuntimeBambuMachineGateway {
    fn redact_error(&self, message: &str) -> String {
        redact_known_access_codes(message, self.redaction_access_codes.lock().unwrap().clone())
    }

    async fn discover_printers(
        &self,
        timeout_seconds: u32,
    ) -> anyhow::Result<crate::machine::discovery::PrinterDiscoveryResult> {
        self.inner
            .lock()
            .await
            .discover_printers(timeout_seconds)
            .await
    }

    async fn diagnose_printer(
        &self,
        serial_number: &str,
    ) -> anyhow::Result<crate::machine::diagnostics::PrinterDiagnosticResult> {
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
    ) -> anyhow::Result<()> {
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

    async fn camera_endpoint(&self, serial_number: &str) -> anyhow::Result<BambuPrinterEndpoint> {
        self.inner.lock().await.camera_endpoint(serial_number).await
    }

    async fn link_printer(
        &self,
        endpoint: BambuPrinterEndpoint,
        _config: &AgentConfig,
        sender: &mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<MachineSnapshot> {
        let command_transport = RumqttcBambuMqttTransport::connect(&endpoint);
        let report_transport = RumqttcBambuMqttTransport::connect_for_reports(&endpoint);
        let transfer = BambuMachineFileTransfer::new(endpoint.clone());
        let mut inner = self.inner.lock().await;
        let snapshot = refresh_printer(&command_transport, &endpoint, self.report_timeout)
            .await
            .with_context(|| format!("validate runtime printer {}", endpoint.serial))?
            .snapshot;
        inner.replace_printer(endpoint.clone(), command_transport, transfer);
        self.record_access_code(&endpoint);
        self.replace_report_task_with_transport(endpoint, report_transport, sender)
            .await;
        Ok(snapshot)
    }
}

#[cfg(test)]
pub(crate) mod test_support;
