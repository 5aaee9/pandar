pub mod brtc;
pub mod camera;
pub mod compatibility;
mod device_features;
pub mod diagnostics;
pub mod discovery;
pub mod file_transfer;
mod firmware;
pub mod ftps;
pub mod materials;
pub mod mqtt;
mod noop;
mod operations;
mod print;
pub mod runtime;
mod transfer;
mod types;
mod unavailable_transfer;

use std::time::Duration;

use crate::{
    AgentConfig,
    protocol::agent::v1::{AgentEvent, PrintProjectFile},
};
use anyhow::bail;
use async_trait::async_trait;
pub use device_features::DeviceFeatureCache;
pub(crate) use device_features::DeviceFeatureLease;
#[cfg(test)]
pub(crate) use device_features::transition_pause as device_feature_transition_pause;
use diagnostics::PrinterDiagnosticResult;
use discovery::{DiscoveredPrinter, PrinterDiscoveryResult};
use file_transfer::{MachineFileTransfer, TransferModeCache};
#[cfg(test)]
pub(crate) use firmware::firmware_event_pause;
pub use firmware::{
    FirmwareCacheSnapshot, FirmwareControlOutcome, FirmwareControlPhase, FirmwareExecuteRequest,
    FirmwareExecutionLease, FirmwareGenerationTransition, FirmwareMachineGateway,
    FirmwareModulesDelivery, FirmwareModulesObservation, FirmwareObservationCache,
    FirmwarePrepareRequest, FirmwarePreparedObservation, FirmwarePublishTransition,
    FirmwareRefreshRequest, FirmwareReportContext, FirmwareReservationState,
    FirmwareStatusObservation, FirmwareVersionObservation, firmware_modules_event,
    firmware_status_event,
};
pub(crate) use firmware::{proto_module as proto_firmware_module, proto_upgrade_state};
pub(crate) use mqtt::report::FirmwareReportReducer;
use mqtt::{BambuMqttTransport, refresh_printer, refresh_printer_materials};
pub use noop::NoopMachineGateway;
use operations::dispatch_printer_operation;
pub use operations::{PrinterAxis, PrinterOperation};
use print::dispatch_print_project_file;
pub(crate) use print::validate_print_project_file_command;
#[cfg(test)]
pub(crate) use runtime::RuntimeReportContext;
use transfer::BambuMachineFileTransfer;
pub use types::{
    BambuPrinterEndpoint, MachineJsonPayload, MachineNozzleTemperature, MachineSnapshot,
    MaterialRefreshResult, PrintProjectDispatchResult, PrinterOperationDispatchResult,
    PrinterOperationMqttSummary, PrinterRefreshResult,
};
pub use unavailable_transfer::UnavailableMachineFileTransfer;

#[async_trait]
pub trait BambuMachineGateway: Send + Sync {
    fn redact_error(&self, message: &str) -> String;
    async fn discover_printers(
        &self,
        timeout_seconds: u32,
    ) -> anyhow::Result<PrinterDiscoveryResult>;
    async fn discover_printer_at_host(
        &self,
        host: &str,
        timeout_seconds: u32,
    ) -> anyhow::Result<Option<DiscoveredPrinter>> {
        discovery::discover_printer_at_host(host, timeout_seconds).await
    }
    async fn diagnose_printer(
        &self,
        serial_number: &str,
    ) -> anyhow::Result<PrinterDiagnosticResult>;
    async fn refresh_printers(&self) -> anyhow::Result<Vec<PrinterRefreshResult>>;
    async fn refresh_printer_materials(
        &self,
        serial_number: &str,
        printer_id: Option<&str>,
    ) -> anyhow::Result<MaterialRefreshResult>;
    async fn validate_printer(&self, serial_number: &str) -> anyhow::Result<()>;
    async fn validate_printer_endpoint_identity(
        &self,
        endpoint: &BambuPrinterEndpoint,
    ) -> anyhow::Result<()> {
        bail!(
            "printer endpoint identity validation is not supported for {}",
            endpoint.serial
        )
    }
    async fn print_project_file(
        &self,
        serial_number: &str,
        command: &PrintProjectFile,
        artifact: Vec<u8>,
    ) -> anyhow::Result<PrintProjectDispatchResult>;
    async fn operate_printer(
        &self,
        serial_number: &str,
        _operation: PrinterOperation,
    ) -> anyhow::Result<PrinterOperationDispatchResult> {
        bail!("no Bambu printer configured for serial {serial_number}")
    }
    async fn camera_endpoint(&self, serial_number: &str) -> anyhow::Result<BambuPrinterEndpoint> {
        bail!("no Bambu printer configured for serial {serial_number}")
    }
    async fn link_printer(
        &self,
        endpoint: BambuPrinterEndpoint,
        config: &AgentConfig,
        sender: &tokio::sync::mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<MachineSnapshot> {
        let _ = (endpoint, config, sender);
        bail!("runtime printer linking is not supported by this gateway")
    }
}

#[derive(Debug)]
pub struct ConfiguredBambuMachineGateway<T, F = BambuMachineFileTransfer> {
    printers: Vec<(BambuPrinterEndpoint, T, F)>,
    report_timeout: Duration,
    transfer_cache: TransferModeCache,
}

impl<T> ConfiguredBambuMachineGateway<T> {
    pub fn new(printers: Vec<(BambuPrinterEndpoint, T)>, report_timeout: Duration) -> Self {
        Self {
            printers: printers
                .into_iter()
                .map(|(endpoint, mqtt)| {
                    let transfer = BambuMachineFileTransfer::new(endpoint.clone());
                    (endpoint, mqtt, transfer)
                })
                .collect(),
            report_timeout,
            transfer_cache: TransferModeCache::default(),
        }
    }
}

#[async_trait]
impl<T, F> BambuMachineGateway for ConfiguredBambuMachineGateway<T, F>
where
    T: BambuMqttTransport + Send + Sync,
    F: MachineFileTransfer + Send + Sync,
{
    fn redact_error(&self, message: &str) -> String {
        diagnostics::PrinterEndpointSecrets::from_endpoints(
            self.printers.iter().map(|(endpoint, _, _)| endpoint),
        )
        .redact(message)
    }

    async fn discover_printers(
        &self,
        timeout_seconds: u32,
    ) -> anyhow::Result<PrinterDiscoveryResult> {
        discovery::discover_printers(timeout_seconds).await
    }

    async fn diagnose_printer(
        &self,
        serial_number: &str,
    ) -> anyhow::Result<PrinterDiagnosticResult> {
        Ok(diagnostics::diagnose_printer(
            &self.printers,
            &self.transfer_cache,
            self.report_timeout,
            serial_number,
        )
        .await)
    }

    async fn refresh_printers(&self) -> anyhow::Result<Vec<PrinterRefreshResult>> {
        let mut snapshots = Vec::with_capacity(self.printers.len());
        for (endpoint, transport, _) in &self.printers {
            snapshots.push(refresh_printer(transport, endpoint, self.report_timeout).await?);
        }
        Ok(snapshots)
    }

    async fn refresh_printer_materials(
        &self,
        serial_number: &str,
        printer_id: Option<&str>,
    ) -> anyhow::Result<MaterialRefreshResult> {
        let Some((endpoint, mqtt, _)) = self
            .printers
            .iter()
            .find(|(endpoint, _, _)| endpoint.serial == serial_number)
        else {
            bail!("no configured Bambu printer matches serial {serial_number}");
        };

        refresh_printer_materials(mqtt, endpoint, printer_id, self.report_timeout).await
    }

    async fn validate_printer(&self, serial_number: &str) -> anyhow::Result<()> {
        if self
            .printers
            .iter()
            .any(|(endpoint, _, _)| endpoint.serial == serial_number)
        {
            return Ok(());
        }

        bail!("no configured Bambu printer matches serial {serial_number}")
    }

    async fn print_project_file(
        &self,
        serial_number: &str,
        command: &PrintProjectFile,
        artifact: Vec<u8>,
    ) -> anyhow::Result<PrintProjectDispatchResult> {
        let Some((endpoint, mqtt, transfer)) = self
            .printers
            .iter()
            .find(|(endpoint, _, _)| endpoint.serial == serial_number)
        else {
            bail!("no configured Bambu printer matches serial {serial_number}");
        };

        dispatch_print_project_file(
            endpoint,
            transfer,
            mqtt,
            &self.transfer_cache,
            command,
            &artifact,
        )
        .await
    }

    async fn operate_printer(
        &self,
        serial_number: &str,
        operation: PrinterOperation,
    ) -> anyhow::Result<PrinterOperationDispatchResult> {
        let Some((endpoint, mqtt, _)) = self
            .printers
            .iter()
            .find(|(endpoint, _, _)| endpoint.serial == serial_number)
        else {
            bail!("no configured Bambu printer matches serial {serial_number}");
        };

        dispatch_printer_operation(endpoint, mqtt, operation, None).await
    }

    async fn camera_endpoint(&self, serial_number: &str) -> anyhow::Result<BambuPrinterEndpoint> {
        let Some((endpoint, _, _)) = self
            .printers
            .iter()
            .find(|(endpoint, _, _)| endpoint.serial == serial_number)
        else {
            bail!("no configured Bambu printer matches serial {serial_number}");
        };
        Ok(endpoint.clone())
    }
}

impl<T, F> ConfiguredBambuMachineGateway<T, F> {
    pub fn configured_printer_count(&self) -> usize {
        self.printers.len()
    }

    pub fn endpoints(&self) -> Vec<BambuPrinterEndpoint> {
        self.printers
            .iter()
            .map(|(endpoint, _, _)| endpoint.clone())
            .collect()
    }

    pub fn endpoint(&self, serial_number: &str) -> Option<BambuPrinterEndpoint> {
        self.printers
            .iter()
            .find(|(endpoint, _, _)| endpoint.serial == serial_number)
            .map(|(endpoint, _, _)| endpoint.clone())
    }

    pub(crate) fn refresh_target(&self, serial_number: &str) -> Option<(BambuPrinterEndpoint, T)>
    where
        T: Clone,
    {
        self.printers
            .iter()
            .find(|(endpoint, _, _)| endpoint.serial == serial_number)
            .map(|(endpoint, transport, _)| (endpoint.clone(), transport.clone()))
    }

    pub(crate) async fn probe_device_features(
        &self,
        serial_number: &str,
        cache: &DeviceFeatureCache,
    ) -> anyhow::Result<pandar_core::BambuDeviceFeatures>
    where
        T: BambuMqttTransport,
    {
        let Some((endpoint, transport, _)) = self
            .printers
            .iter()
            .find(|(endpoint, _, _)| endpoint.serial == serial_number)
        else {
            bail!("no configured Bambu printer matches serial {serial_number}");
        };
        mqtt::probe_device_features(transport, endpoint, self.report_timeout, cache).await
    }

    pub(crate) async fn operate_printer_with_device_feature_lease(
        &self,
        serial_number: &str,
        operation: PrinterOperation,
        lease: Option<DeviceFeatureLease>,
    ) -> anyhow::Result<PrinterOperationDispatchResult>
    where
        T: BambuMqttTransport + Send + Sync,
    {
        let Some((endpoint, mqtt, _)) = self
            .printers
            .iter()
            .find(|(endpoint, _, _)| endpoint.serial == serial_number)
        else {
            bail!("no configured Bambu printer matches serial {serial_number}");
        };

        dispatch_printer_operation(endpoint, mqtt, operation, lease).await
    }

    pub fn replace_printer(&mut self, endpoint: BambuPrinterEndpoint, mqtt: T, transfer: F) {
        self.printers
            .retain(|(existing, _, _)| existing.serial != endpoint.serial);
        self.printers.push((endpoint, mqtt, transfer));
    }

    pub fn with_file_transfer(
        printers: Vec<(BambuPrinterEndpoint, T, F)>,
        report_timeout: Duration,
        transfer_cache: TransferModeCache,
    ) -> Self {
        Self {
            printers,
            report_timeout,
            transfer_cache,
        }
    }
}

#[cfg(test)]
mod tests;
