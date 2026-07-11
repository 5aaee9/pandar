use std::{collections::HashMap, sync::Mutex as StdMutex, time::Duration};

use tokio::{sync::mpsc, task::JoinHandle, time::sleep};
use {anyhow::Context, async_trait::async_trait};

use crate::{
    AgentConfig,
    machine::{
        BambuMachineGateway, BambuPrinterEndpoint, ConfiguredBambuMachineGateway,
        DeviceFeatureCache, MachineSnapshot, MaterialRefreshResult, PrintProjectDispatchResult,
        PrinterOperation, PrinterOperationDispatchResult, PrinterRefreshResult,
        diagnostics::{redact_access_code, redact_known_access_codes},
        mqtt::{
            RumqttcBambuMqttTransport, dispatch_sequence_zero_recovery, feature_event,
            forward_print_reports, refresh_printer,
        },
        transfer::BambuMachineFileTransfer,
    },
    protocol::agent::v1::{AgentEvent, PrintProjectFile},
};

use super::operations::mqtt_command_for_printer_operation;
use super::operations::operate_printer_with_feature_selection;

pub struct RuntimeBambuMachineGateway {
    inner: tokio::sync::Mutex<ConfiguredBambuMachineGateway<RumqttcBambuMqttTransport>>,
    report_tasks: tokio::sync::Mutex<HashMap<String, JoinHandle<()>>>,
    device_features: DeviceFeatureCache,
    current_sender: tokio::sync::Mutex<Option<mpsc::Sender<AgentEvent>>>,
    redaction_access_codes: StdMutex<Vec<String>>,
    config: AgentConfig,
    report_timeout: Duration,
}

const REPORT_FORWARD_RETRY_DELAY: Duration = Duration::from_secs(5);

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
            device_features: DeviceFeatureCache::default(),
            current_sender: tokio::sync::Mutex::new(None),
            redaction_access_codes: StdMutex::new(redaction_access_codes),
            config,
            report_timeout,
        }
    }

    pub async fn prepare_session(&self, sender: &mpsc::Sender<AgentEvent>) -> anyhow::Result<()> {
        let tasks = self
            .report_tasks
            .lock()
            .await
            .drain()
            .map(|(_, task)| task)
            .collect::<Vec<_>>();
        for task in tasks {
            task.abort();
            let _ = task.await;
        }
        *self.current_sender.lock().await = Some(sender.clone());
        let endpoints = self.inner.lock().await.endpoints();
        for endpoint in &endpoints {
            self.device_features.invalidate(&endpoint.serial).await;
            sender
                .send(feature_event(&self.config, endpoint.serial.clone(), None))
                .await
                .with_context(|| {
                    format!(
                        "queue printer {} device feature invalidation",
                        endpoint.serial
                    )
                })?;
        }
        for endpoint in &endpoints {
            let observation = self
                .inner
                .lock()
                .await
                .probe_device_features(&endpoint.serial, &self.device_features)
                .await;
            match observation {
                Ok(value) => sender
                    .send(feature_event(
                        &self.config,
                        endpoint.serial.clone(),
                        Some(value),
                    ))
                    .await
                    .with_context(|| {
                        format!(
                            "queue printer {} device feature observation",
                            endpoint.serial
                        )
                    })?,
                Err(error) => {
                    let error = self.redact_error(&format!("{error:#}"));
                    tracing::warn!(
                        serial = %endpoint.serial,
                        error = %error,
                        "printer device feature probe failed"
                    );
                }
            }
        }
        self.start_initial_report_forwarders(sender).await;
        Ok(())
    }

    pub async fn clear_session_sender(&self, sender: &mpsc::Sender<AgentEvent>) {
        let mut current = self.current_sender.lock().await;
        if current
            .as_ref()
            .is_some_and(|current| current.same_channel(sender))
        {
            *current = None;
        }
    }

    pub fn device_feature_cache(&self) -> DeviceFeatureCache {
        self.device_features.clone()
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
        tokio::spawn(forward_print_reports_with_retry(
            config,
            transport,
            endpoint,
            report_timeout,
            sender,
            REPORT_FORWARD_RETRY_DELAY,
            self.device_features.clone(),
        ))
    }

    fn record_access_code(&self, endpoint: &BambuPrinterEndpoint) {
        let mut access_codes = self.redaction_access_codes.lock().unwrap();
        access_codes.retain(|access_code| access_code != &endpoint.access_code);
        access_codes.push(endpoint.access_code.clone());
    }
}

pub(crate) async fn forward_print_reports_with_retry<T>(
    config: AgentConfig,
    transport: T,
    endpoint: BambuPrinterEndpoint,
    report_timeout: Duration,
    sender: mpsc::Sender<AgentEvent>,
    retry_delay: Duration,
    cache: DeviceFeatureCache,
) where
    T: crate::machine::mqtt::BambuMqttTransport + Send + Sync,
{
    loop {
        match forward_print_reports(
            &config,
            &transport,
            &endpoint,
            report_timeout,
            &sender,
            &cache,
        )
        .await
        {
            Ok(()) => return,
            Err(err) => {
                let error = redact_access_code(&format!("{err:#}"), &endpoint.access_code);
                tracing::warn!(
                    serial = %endpoint.serial,
                    error = %error,
                    "printer report forwarding failed; retrying"
                );
                cache.invalidate(&endpoint.serial).await;
                if sender
                    .send(feature_event(&config, endpoint.serial.clone(), None))
                    .await
                    .is_err()
                {
                    return;
                }
            }
        }

        tokio::select! {
            _ = sender.closed() => return,
            _ = sleep(retry_delay) => {}
        }
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
        let results = self.inner.lock().await.refresh_printers().await?;
        for result in &results {
            if let Some(value) = result.snapshot.device_features {
                self.device_features
                    .update(&result.snapshot.serial, value)
                    .await;
            }
        }
        Ok(results)
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
        if matches!(
            &operation,
            PrinterOperation::HandlePrintError { sequence_id: 0, .. }
        ) {
            let endpoint = self
                .inner
                .lock()
                .await
                .endpoint(serial_number)
                .with_context(|| {
                    format!("no configured Bambu printer matches serial {serial_number}")
                })?;
            let command = mqtt_command_for_printer_operation(operation)?;
            return dispatch_sequence_zero_recovery(&endpoint, command).await;
        }

        operate_printer_with_feature_selection(
            &self.config,
            &self.inner,
            &self.device_features,
            &self.current_sender,
            serial_number,
            operation,
        )
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
        if let Some(task) = self.report_tasks.lock().await.remove(&endpoint.serial) {
            task.abort();
            let _ = task.await;
        }
        self.device_features.invalidate(&endpoint.serial).await;
        sender
            .send(feature_event(&self.config, endpoint.serial.clone(), None))
            .await
            .with_context(|| {
                format!(
                    "queue printer {} device feature invalidation",
                    endpoint.serial
                )
            })?;
        if let Some(value) = snapshot.device_features {
            self.device_features.update(&endpoint.serial, value).await;
        }
        inner.replace_printer(endpoint.clone(), command_transport, transfer);
        self.record_access_code(&endpoint);
        self.replace_report_task_with_transport(endpoint, report_transport, sender)
            .await;
        Ok(snapshot)
    }
}

#[cfg(test)]
pub(crate) mod test_support;
