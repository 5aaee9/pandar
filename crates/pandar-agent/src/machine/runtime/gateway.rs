use anyhow::Context;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::{
    AgentConfig,
    commands::authoritative_printer_snapshot_event,
    machine::{
        BambuMachineGateway, BambuPrinterEndpoint, FirmwareReportContext, MachineSnapshot,
        MaterialRefreshResult, PrintProjectDispatchResult, PrinterOperation,
        PrinterOperationDispatchResult, PrinterRefreshResult,
        diagnostics::redact_known_access_codes,
        firmware_modules_event,
        mqtt::{
            RumqttcBambuMqttTransport, dispatch_sequence_zero_recovery, feature_event,
            refresh_printer_with_firmware, resolve_bambu_mqtt_serial,
        },
        transfer::BambuMachineFileTransfer,
    },
    protocol::agent::v1::{AgentEvent, PrintProjectFile},
};

use super::{
    RuntimeBambuMachineGateway, mqtt_command_for_printer_operation,
    operate_printer_with_feature_selection, refresh_runtime_printers_with_firmware,
};

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
        let event_context = self
            .current_sender
            .lock()
            .await
            .clone()
            .map(|sender| (self.config.clone(), sender));
        refresh_runtime_printers_with_firmware(
            std::sync::Arc::clone(&self.inner),
            self.firmware.clone(),
            self.device_features.clone(),
            event_context,
            self.report_timeout,
        )
        .await
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

    async fn validate_printer_endpoint_identity(
        &self,
        endpoint: &BambuPrinterEndpoint,
    ) -> anyhow::Result<()> {
        let actual_serial = resolve_bambu_mqtt_serial(&endpoint.host).await?;
        if actual_serial != endpoint.serial {
            anyhow::bail!(
                "printer at {} reported serial {actual_serial}, expected {}",
                endpoint.host,
                endpoint.serial
            );
        }
        Ok(())
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
        let version_lease = self
            .firmware
            .version_observation_lease(&endpoint.serial)
            .await;
        #[cfg(test)]
        let validation = match self.link_validation_result.lock().await.take() {
            Some(validation) => validation,
            None => {
                refresh_printer_with_firmware(&command_transport, &endpoint, self.report_timeout)
                    .await
            }
        };
        #[cfg(not(test))]
        let validation =
            refresh_printer_with_firmware(&command_transport, &endpoint, self.report_timeout).await;
        let (refresh, firmware_observation) =
            validation.with_context(|| format!("validate runtime printer {}", endpoint.serial))?;
        let snapshot = refresh.snapshot;
        self.stop_report_task(
            &endpoint.serial,
            "join runtime printer report forwarder before endpoint replacement",
        )
        .await?;
        let replaces_generation = self.firmware.snapshot(&endpoint.serial).await.is_some();
        if !replaces_generation {
            sender
                .send(authoritative_printer_snapshot_event(
                    &self.config,
                    snapshot.clone(),
                ))
                .await
                .context("queue linked printer snapshot event")?;
        }
        let transition = self
            .firmware
            .begin_generation(&self.config, endpoint.clone(), sender, None)
            .await?
            .expect("endpoint replacement generation is unconditional");
        if replaces_generation {
            sender
                .send(authoritative_printer_snapshot_event(
                    &self.config,
                    snapshot.clone(),
                ))
                .await
                .context("queue linked printer snapshot event")?;
            transition.resend_invalidation(&self.config, sender).await?;
        }
        let mut inner = self.inner.lock().await;
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
        let firmware = FirmwareReportContext {
            cache: self.firmware.clone(),
            generation: transition.generation(),
        };
        self.replace_report_task_with_transport(
            endpoint.clone(),
            report_transport,
            sender,
            firmware,
        )
        .await?;
        let modules = transition.commit_modules(&endpoint.serial, firmware_observation.modules)?;
        sender
            .send(firmware_modules_event(&self.config, modules))
            .await
            .with_context(|| {
                format!(
                    "queue printer {} link-validation firmware modules",
                    endpoint.serial
                )
            })?;
        drop(transition);
        drop(version_lease);
        Ok(snapshot)
    }
}
