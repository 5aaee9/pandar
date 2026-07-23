use std::{
    collections::HashMap,
    sync::{Arc, Mutex as StdMutex},
    time::Duration,
};

use anyhow::Context;
#[cfg(test)]
use tokio::time::sleep;
use tokio::{sync::mpsc, task::JoinHandle};

#[cfg(test)]
use crate::machine::{
    FirmwareVersionObservation, PrinterRefreshResult, diagnostics::redact_access_code,
    mqtt::forward_print_reports,
};
use crate::{
    AgentConfig,
    machine::{
        BambuMachineGateway, BambuPrinterEndpoint, ConfiguredBambuMachineGateway,
        DeviceFeatureCache, FirmwareObservationCache, FirmwareReportContext,
        diagnostics::PrinterEndpointSecrets,
        mqtt::{FirmwareMqttTaskSet, RumqttcBambuMqttTransport, feature_event},
    },
    protocol::agent::v1::AgentEvent,
};

use super::operations::mqtt_command_for_printer_operation;
use super::operations::operate_printer_with_feature_selection;

mod firmware;
mod firmware_gateway;
#[cfg(test)]
mod firmware_gateway_tests;
mod firmware_refresh;
#[cfg(test)]
mod firmware_refresh_tests;
#[cfg(test)]
mod firmware_tests;
mod gateway;
mod session;
#[cfg(test)]
mod session_test_support;
pub(crate) use firmware::{
    RuntimeReportContext, forward_print_reports_with_firmware_retry,
    refresh_runtime_printers_with_firmware,
};

#[cfg(test)]
type LinkValidationResult = anyhow::Result<(PrinterRefreshResult, FirmwareVersionObservation)>;

pub struct RuntimeBambuMachineGateway {
    inner: Arc<tokio::sync::Mutex<ConfiguredBambuMachineGateway<RumqttcBambuMqttTransport>>>,
    report_tasks: tokio::sync::Mutex<HashMap<String, JoinHandle<()>>>,
    device_features: DeviceFeatureCache,
    firmware: FirmwareObservationCache,
    firmware_mqtt_tasks: FirmwareMqttTaskSet,
    current_sender: tokio::sync::Mutex<Option<mpsc::Sender<AgentEvent>>>,
    redaction_values: StdMutex<PrinterEndpointSecrets>,
    config: AgentConfig,
    report_timeout: Duration,
    #[cfg(test)]
    prepare_session_pause: tokio::sync::Mutex<Option<Arc<PrepareSessionPauseState>>>,
    #[cfg(test)]
    partial_prepare_report_hook:
        tokio::sync::Mutex<Option<Arc<session_test_support::PartialPrepareReportHookState>>>,
    #[cfg(test)]
    report_join_pause: tokio::sync::Mutex<Option<Arc<session_test_support::ReportJoinPauseState>>>,
    #[cfg(test)]
    heartbeat_panic_hook:
        tokio::sync::Mutex<Option<Arc<session_test_support::HeartbeatPanicState>>>,
    #[cfg(test)]
    link_validation_result: tokio::sync::Mutex<Option<LinkValidationResult>>,
}

const REPORT_FORWARD_RETRY_DELAY: Duration = Duration::from_secs(5);

impl RuntimeBambuMachineGateway {
    pub fn new(
        config: AgentConfig,
        printers: Vec<BambuPrinterEndpoint>,
        report_timeout: Duration,
    ) -> Self {
        let redaction_values = PrinterEndpointSecrets::from_endpoints(&printers);
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
            inner: Arc::new(tokio::sync::Mutex::new(inner)),
            report_tasks: tokio::sync::Mutex::new(HashMap::new()),
            device_features: DeviceFeatureCache::default(),
            firmware: FirmwareObservationCache::default(),
            firmware_mqtt_tasks: FirmwareMqttTaskSet::default(),
            current_sender: tokio::sync::Mutex::new(None),
            redaction_values: StdMutex::new(redaction_values),
            config,
            report_timeout,
            #[cfg(test)]
            prepare_session_pause: tokio::sync::Mutex::new(None),
            #[cfg(test)]
            partial_prepare_report_hook: tokio::sync::Mutex::new(None),
            #[cfg(test)]
            report_join_pause: tokio::sync::Mutex::new(None),
            #[cfg(test)]
            heartbeat_panic_hook: tokio::sync::Mutex::new(None),
            #[cfg(test)]
            link_validation_result: tokio::sync::Mutex::new(None),
        }
    }

    pub async fn prepare_session(&self, sender: &mpsc::Sender<AgentEvent>) -> anyhow::Result<()> {
        self.teardown_session_report_forwarders().await?;
        *self.current_sender.lock().await = Some(sender.clone());
        #[cfg(test)]
        self.pause_prepare_session_for_test_if_installed().await;
        let endpoints = self.inner.lock().await.endpoints();
        #[cfg(test)]
        self.fail_partial_prepare_after_report_forwarder_for_test_if_installed(&endpoints, sender)
            .await?;
        self.queue_configured_printer_rows(&endpoints, sender)
            .await?;
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
        self.start_initial_report_forwarders(sender).await?;
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

    #[cfg(test)]
    pub(crate) async fn pause_prepare_session_for_test(&self) -> PrepareSessionPause {
        let state = Arc::new(PrepareSessionPauseState {
            reached: tokio::sync::Notify::new(),
            dropped: tokio::sync::Notify::new(),
        });
        *self.prepare_session_pause.lock().await = Some(Arc::clone(&state));
        PrepareSessionPause { state }
    }

    #[cfg(test)]
    async fn pause_prepare_session_for_test_if_installed(&self) {
        let Some(state) = self.prepare_session_pause.lock().await.take() else {
            return;
        };
        let _guard = PrepareSessionPauseGuard(Arc::clone(&state));
        state.reached.notify_one();
        std::future::pending::<()>().await;
    }

    #[cfg(test)]
    pub(crate) async fn has_current_sender_for_test(&self) -> bool {
        self.current_sender.lock().await.is_some()
    }

    pub fn device_feature_cache(&self) -> DeviceFeatureCache {
        self.device_features.clone()
    }

    pub fn firmware_cache(&self) -> FirmwareObservationCache {
        self.firmware.clone()
    }

    pub async fn start_initial_report_forwarders(
        &self,
        sender: &mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<()> {
        let endpoints = self.inner.lock().await.endpoints();
        for endpoint in endpoints {
            self.replace_report_task(endpoint, sender).await?;
        }
        Ok(())
    }

    async fn replace_report_task(
        &self,
        endpoint: BambuPrinterEndpoint,
        sender: &mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<()> {
        let transport = RumqttcBambuMqttTransport::connect_for_reports(&endpoint);
        let transition = self
            .firmware
            .begin_generation(&self.config, endpoint.clone(), sender, None)
            .await?
            .expect("initial report generation is unconditional");
        let firmware = FirmwareReportContext {
            cache: self.firmware.clone(),
            generation: transition.generation(),
        };
        self.replace_report_task_with_transport(endpoint, transport, sender, firmware)
            .await?;
        drop(transition);
        Ok(())
    }

    pub(super) async fn replace_report_task_with_transport(
        &self,
        endpoint: BambuPrinterEndpoint,
        transport: RumqttcBambuMqttTransport,
        sender: &mpsc::Sender<AgentEvent>,
        firmware: FirmwareReportContext,
    ) -> anyhow::Result<()> {
        let serial = endpoint.serial.clone();
        self.stop_report_task(&serial, "join replaced runtime printer report forwarder")
            .await?;
        let mut tasks = self.report_tasks.lock().await;
        let previous = tasks.insert(
            serial.clone(),
            self.spawn_report_task(endpoint, transport, sender.clone(), firmware),
        );
        assert!(previous.is_none(), "report task replacement is serialized");
        Ok(())
    }

    fn spawn_report_task(
        &self,
        endpoint: BambuPrinterEndpoint,
        transport: RumqttcBambuMqttTransport,
        sender: mpsc::Sender<AgentEvent>,
        firmware: FirmwareReportContext,
    ) -> JoinHandle<()> {
        let config = self.config.clone();
        let report_timeout = self.report_timeout;
        tokio::spawn(forward_print_reports_with_firmware_retry(
            config,
            transport,
            endpoint,
            report_timeout,
            sender,
            REPORT_FORWARD_RETRY_DELAY,
            RuntimeReportContext {
                device_features: self.device_features.clone(),
                firmware,
            },
        ))
    }

    fn record_endpoint_secrets(&self, endpoint: &BambuPrinterEndpoint) {
        self.redaction_values.lock().unwrap().record(endpoint);
    }
}

#[cfg(test)]
pub(crate) struct PrepareSessionPause {
    state: Arc<PrepareSessionPauseState>,
}

#[cfg(test)]
struct PrepareSessionPauseState {
    reached: tokio::sync::Notify,
    dropped: tokio::sync::Notify,
}

#[cfg(test)]
struct PrepareSessionPauseGuard(Arc<PrepareSessionPauseState>);

#[cfg(test)]
impl PrepareSessionPause {
    pub(crate) async fn wait_until_reached(&mut self) {
        self.state.reached.notified().await;
    }

    pub(crate) async fn wait_until_dropped(&mut self) {
        self.state.dropped.notified().await;
    }
}

#[cfg(test)]
impl Drop for PrepareSessionPauseGuard {
    fn drop(&mut self) {
        self.0.dropped.notify_one();
    }
}

#[cfg(test)]
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

#[cfg(test)]
pub(crate) mod test_support;
