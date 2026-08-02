use std::time::Duration;

mod commands;
mod device_features;
mod fake;
mod firmware;
mod hms;
mod recovery;
pub(crate) mod report;
mod report_payload;
mod reports;
mod signing;
mod snapshot;
mod transport;

use anyhow::{Context, anyhow, bail};
use async_trait::async_trait;
use pandar_core::created_at_now;
use serde_json::Value;

#[cfg(test)]
pub(crate) use crate::{machine::MachineSnapshot, protocol::agent::v1::AgentEvent};
pub(crate) use commands::chamber_light_commands_for_nodes;
#[cfg(test)]
pub(crate) use commands::next_studio_sequence_id_from;
pub use commands::{
    AmsFilamentCommand, AmsSlotCommand, BambuMqttCommand, BambuMqttTopics, GcodeLineCommand,
    HandlePrintErrorCommand, MachineReportDiagnostic, MachineReportDiagnosticPayload,
    PrintErrorAction, PrintReportProgress, PrintSpeed, ProjectFileAmsMapping2,
    ProjectFileAmsMappingInfo, ProjectFileCommand, SetNozzleTemperatureCommand,
};
pub(crate) use device_features::{feature_event, probe_device_features};
#[cfg(test)]
pub(crate) use fake::FakeMqttTransport;
pub(crate) use firmware::{
    FirmwareMqttCommand, FirmwareMqttSession, FirmwareMqttTaskSet, FirmwarePumpAbortHandle,
    firmware_command_payload, firmware_mqtt_failure, firmware_mqtt_failure_phase,
};
#[cfg(test)]
pub(crate) use firmware::{
    firmware_barrier_pause, firmware_mqtt_options, firmware_pump_drop_pause,
    is_firmware_pre_publish_failure,
};
pub use hms::MachineHmsItem;
pub(super) use recovery::dispatch_sequence_zero_recovery;
pub(crate) use report::{MachineReport, MachineReports, device_feature_observation};
pub(crate) use report_payload::decode_mqtt_report_payload;
pub(crate) use reports::{
    MqttForwardingContext, MqttPresenceState, forward_print_reports_with_context,
    printer_materials_snapshot_event,
};
#[cfg(test)]
pub(crate) use reports::{
    forward_print_reports, forward_print_reports_with_firmware, print_job_report_event,
    print_report_from_report,
};
#[cfg(test)]
pub(crate) use rumqttc::TlsConfiguration;
#[cfg(test)]
pub(crate) use snapshot::snapshot_from_report;
pub(crate) use snapshot::{nozzle_system_patch_from_report, snapshot_from_parsed_report};
pub(crate) use transport::BambuLanCertificateVerifier;
#[cfg(test)]
pub(crate) use transport::mqtt_report_idle_timeout;
#[cfg(test)]
pub(crate) use transport::warn_mqtt_report_receive_failed;
pub use transport::{RumqttcBambuMqttTransport, bambu_lan_mqtt_options, bambu_lan_tls_config};
pub(crate) use transport::{
    bambu_lan_client_config, is_mqtt_report_idle_timeout, resolve_bambu_mqtt_serial,
};

use crate::machine::{
    BambuPrinterEndpoint, FirmwareVersionObservation, MaterialRefreshResult, PrinterRefreshResult,
    materials::normalize_material_patch,
};

pub const BAMBU_MQTT_PORT: u16 = 8883;
pub const BAMBU_MQTT_USERNAME: &str = "bblp";
pub const BAMBU_MQTT_QOS: u8 = 1;
pub const BAMBU_MQTT_RETAIN: bool = false;
const BAMBU_MQTT_MAX_PACKET_SIZE: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct PublishedMqttCommand {
    pub topic: String,
    pub payload: Value,
    pub qos: u8,
}

#[async_trait]
pub trait BambuMqttTransport: Send + Sync {
    async fn subscribe(&self, topic: &str) -> anyhow::Result<()>;
    async fn publish(&self, command: PublishedMqttCommand) -> anyhow::Result<()>;
    async fn next_report(&self, timeout: Duration) -> anyhow::Result<Value>;
}

pub async fn refresh_printer<T>(
    transport: &T,
    endpoint: &BambuPrinterEndpoint,
    report_timeout: Duration,
) -> anyhow::Result<PrinterRefreshResult>
where
    T: BambuMqttTransport + ?Sized,
{
    refresh_printer_with_firmware(transport, endpoint, report_timeout)
        .await
        .map(|(refresh, _)| refresh)
}

pub(crate) async fn refresh_printer_with_firmware<T>(
    transport: &T,
    endpoint: &BambuPrinterEndpoint,
    report_timeout: Duration,
) -> anyhow::Result<(PrinterRefreshResult, FirmwareVersionObservation)>
where
    T: BambuMqttTransport + ?Sized,
{
    async move {
        let reports = MachineReports::new(transport);
        let topics = BambuMqttTopics::for_serial(&endpoint.serial);
        reports
            .subscribe(&topics.report)
            .await
            .with_context(|| format!("subscribe to report topic {}", topics.report))?;
        let firmware = discover_firmware_version(&reports, &topics, report_timeout)
            .await
            .inspect_err(|err| {
                tracing::warn!(
                    serial = %endpoint.serial,
                    error = %format!("{err:#}"),
                    "printer model discovery failed"
                );
            })?;
        reports
            .publish(PublishedMqttCommand {
                topic: topics.request.clone(),
                payload: BambuMqttCommand::RequestPushAll.payload(),
                qos: BAMBU_MQTT_QOS,
            })
            .await
            .with_context(|| format!("publish pushall to request topic {}", topics.request))?;
        let material_deadline = tokio::time::Instant::now() + report_timeout;
        let report = reports
            .next_report(report_timeout)
            .await
            .context("wait for MQTT report")?;
        if let Some(snapshot_report) = report.snapshot()
            && let Err(error) = device_feature_observation(&endpoint.serial, snapshot_report)
        {
            tracing::warn!(
                serial = %endpoint.serial,
                error = %format!("{error:#}"),
                "invalid printer device feature observation during refresh"
            );
        }
        let mut snapshot = snapshot_from_parsed_report(endpoint, report.snapshot());
        snapshot.model = Some(firmware.model.clone());
        snapshot.telemetry_authoritative = true;
        let observed_at = created_at_now();
        let materials = match report
            .materials()
            .and_then(|report| normalize_material_patch(report, &observed_at))
        {
            Some(patch) => Some(MaterialRefreshResult {
                serial: endpoint.serial.clone(),
                printer_id: None,
                printer_materials_json: serde_json::to_string(&patch)
                    .context("encode printer materials patch")?,
            }),
            None => scan_materials_after_snapshot(&reports, endpoint, material_deadline).await?,
        };
        Ok::<_, anyhow::Error>((
            PrinterRefreshResult {
                snapshot,
                materials,
            },
            firmware,
        ))
    }
    .await
    .with_context(|| format!("refresh printer {}", endpoint.serial))
}

async fn scan_materials_after_snapshot<T>(
    reports: &MachineReports<'_, T>,
    endpoint: &BambuPrinterEndpoint,
    deadline: tokio::time::Instant,
) -> anyhow::Result<Option<MaterialRefreshResult>>
where
    T: BambuMqttTransport + ?Sized,
{
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Ok(None);
        }
        let remaining = deadline.saturating_duration_since(now);
        let report = match reports.next_report(remaining).await {
            Ok(report) => report,
            Err(err) => {
                tracing::warn!(
                    serial = %endpoint.serial,
                    error = %format!("{err:#}"),
                    "printer material refresh after snapshot ended without AMS report"
                );
                return Ok(None);
            }
        };
        let observed_at = created_at_now();
        if let Some(patch) = report
            .materials()
            .and_then(|report| normalize_material_patch(report, &observed_at))
        {
            return Ok(Some(MaterialRefreshResult {
                serial: endpoint.serial.clone(),
                printer_id: None,
                printer_materials_json: serde_json::to_string(&patch)
                    .context("encode printer materials patch")?,
            }));
        }
    }
}

pub async fn refresh_printer_materials<T>(
    transport: &T,
    endpoint: &BambuPrinterEndpoint,
    printer_id: Option<&str>,
    report_timeout: Duration,
) -> anyhow::Result<MaterialRefreshResult>
where
    T: BambuMqttTransport + ?Sized,
{
    let reports = MachineReports::new(transport);
    let topics = BambuMqttTopics::for_serial(&endpoint.serial);
    reports
        .subscribe(&topics.report)
        .await
        .with_context(|| format!("subscribe to report topic {}", topics.report))?;
    reports
        .publish(PublishedMqttCommand {
            topic: topics.request.clone(),
            payload: BambuMqttCommand::RequestPushAll.payload(),
            qos: BAMBU_MQTT_QOS,
        })
        .await
        .with_context(|| format!("publish pushall to request topic {}", topics.request))?;

    let deadline = tokio::time::Instant::now() + report_timeout;
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            bail!("no AMS material report received before timeout");
        }
        let remaining = deadline.saturating_duration_since(now);
        let report = match reports.next_report(remaining).await {
            Ok(report) => report,
            Err(err) if tokio::time::Instant::now() >= deadline => {
                return Err(err).context("no AMS material report received before timeout");
            }
            Err(err) => return Err(err),
        };
        let observed_at = created_at_now();
        if let Some(patch) = report
            .materials()
            .and_then(|report| normalize_material_patch(report, &observed_at))
        {
            return Ok(MaterialRefreshResult {
                serial: endpoint.serial.clone(),
                printer_id: printer_id.map(str::to_owned),
                printer_materials_json: serde_json::to_string(&patch)
                    .context("encode printer materials patch")?,
            });
        }
    }
}

async fn discover_firmware_version<T>(
    reports: &MachineReports<'_, T>,
    topics: &BambuMqttTopics,
    report_timeout: Duration,
) -> anyhow::Result<FirmwareVersionObservation>
where
    T: BambuMqttTransport + ?Sized,
{
    reports
        .publish(PublishedMqttCommand {
            topic: topics.request.clone(),
            payload: BambuMqttCommand::GetVersion.payload(),
            qos: BAMBU_MQTT_QOS,
        })
        .await
        .with_context(|| format!("publish get_version to request topic {}", topics.request))?;

    tokio::time::timeout(report_timeout, async {
        loop {
            let report = reports
                .next_report(report_timeout)
                .await
                .context("wait for MQTT get_version report")?;
            if let Some(observation) = report.firmware_version_observation()? {
                return Ok(observation);
            }
        }
    })
    .await
    .map_err(|_| {
        anyhow!("timed out waiting for MQTT get_version report after {report_timeout:?}")
    })?
}

#[cfg(test)]
mod tests;
