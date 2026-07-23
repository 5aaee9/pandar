use std::time::Duration;

mod commands;
mod device_features;
mod fake;
mod firmware;
mod hms;
mod recovery;
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
pub(crate) use device_features::{
    device_feature_observation, feature_event, probe_device_features,
};
#[cfg(test)]
pub(crate) use fake::FakeMqttTransport;
pub(crate) use firmware::{
    FirmwareMqttCommand, FirmwareMqttSession, FirmwareMqttTaskSet, FirmwarePumpAbortHandle,
    firmware_command_payload, firmware_mqtt_failure, firmware_mqtt_failure_phase,
    has_non_firmware_print_telemetry, parse_firmware_acknowledgement,
    parse_firmware_refresh_modules, parse_firmware_version_observation,
};
#[cfg(test)]
pub(crate) use firmware::{
    firmware_barrier_pause, firmware_mqtt_options, firmware_pump_drop_pause,
    is_firmware_pre_publish_failure,
};
pub use hms::MachineHmsItem;
pub(super) use recovery::dispatch_sequence_zero_recovery;
pub(crate) use report_payload::decode_mqtt_report_payload;
pub(crate) use reports::{
    MqttForwardingContext, MqttPresenceState, forward_print_reports_with_context,
};
pub use reports::{
    forward_print_reports, forward_print_reports_with_firmware, print_job_report_event,
    print_report_from_report, printer_materials_snapshot_event,
};
#[cfg(test)]
pub(crate) use rumqttc::TlsConfiguration;
pub use snapshot::snapshot_from_report;
pub(crate) use snapshot::{SnapshotReport, parse_snapshot_report, snapshot_from_parsed_report};
pub(crate) use transport::BambuLanCertificateVerifier;
#[cfg(test)]
pub(crate) use transport::mqtt_report_idle_timeout;
#[cfg(test)]
pub(crate) use transport::warn_mqtt_report_receive_failed;
pub use transport::{RumqttcBambuMqttTransport, bambu_lan_mqtt_options, bambu_lan_tls_config};
pub(crate) use transport::{is_mqtt_report_idle_timeout, resolve_bambu_mqtt_serial};

use crate::machine::{
    BambuPrinterEndpoint, FirmwareVersionObservation, MaterialRefreshResult, PrinterRefreshResult,
    materials::{normalize_material_patch, parse_materials_report},
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
        let topics = BambuMqttTopics::for_serial(&endpoint.serial);
        transport
            .subscribe(&topics.report)
            .await
            .with_context(|| format!("subscribe to report topic {}", topics.report))?;
        let firmware = discover_firmware_version(transport, &topics, report_timeout)
            .await
            .inspect_err(|err| {
                tracing::warn!(
                    serial = %endpoint.serial,
                    error = %format!("{err:#}"),
                    "printer model discovery failed"
                );
            })?;
        transport
            .publish(PublishedMqttCommand {
                topic: topics.request.clone(),
                payload: BambuMqttCommand::RequestPushAll.payload(),
                qos: BAMBU_MQTT_QOS,
            })
            .await
            .with_context(|| format!("publish pushall to request topic {}", topics.request))?;
        let material_deadline = tokio::time::Instant::now() + report_timeout;
        let report = transport
            .next_report(report_timeout)
            .await
            .context("wait for MQTT report")?;
        let snapshot_report = parse_snapshot_report(&report);
        if let Some(snapshot_report) = snapshot_report.as_ref()
            && let Err(error) = device_feature_observation(&endpoint.serial, snapshot_report)
        {
            tracing::warn!(
                serial = %endpoint.serial,
                error = %format!("{error:#}"),
                "invalid printer device feature observation during refresh"
            );
        }
        let mut snapshot = snapshot_from_parsed_report(endpoint, snapshot_report.as_ref());
        snapshot.model = Some(firmware.model.clone());
        snapshot.telemetry_authoritative = true;
        let observed_at = created_at_now();
        let materials_report = parse_materials_report(&report);
        let materials = match materials_report
            .as_ref()
            .and_then(|report| normalize_material_patch(report, &observed_at))
        {
            Some(patch) => Some(MaterialRefreshResult {
                serial: endpoint.serial.clone(),
                printer_id: None,
                printer_materials_json: serde_json::to_string(&patch)
                    .context("encode printer materials patch")?,
            }),
            None => scan_materials_after_snapshot(transport, endpoint, material_deadline).await?,
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
    transport: &T,
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
        let report = match transport.next_report(remaining).await {
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
        let materials_report = parse_materials_report(&report);
        if let Some(patch) = materials_report
            .as_ref()
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
    let topics = BambuMqttTopics::for_serial(&endpoint.serial);
    transport
        .subscribe(&topics.report)
        .await
        .with_context(|| format!("subscribe to report topic {}", topics.report))?;
    transport
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
        let report = match transport.next_report(remaining).await {
            Ok(report) => report,
            Err(err) if tokio::time::Instant::now() >= deadline => {
                return Err(err).context("no AMS material report received before timeout");
            }
            Err(err) => return Err(err),
        };
        let observed_at = created_at_now();
        let materials_report = parse_materials_report(&report);
        if let Some(patch) = materials_report
            .as_ref()
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
    transport: &T,
    topics: &BambuMqttTopics,
    report_timeout: Duration,
) -> anyhow::Result<FirmwareVersionObservation>
where
    T: BambuMqttTransport + ?Sized,
{
    transport
        .publish(PublishedMqttCommand {
            topic: topics.request.clone(),
            payload: BambuMqttCommand::GetVersion.payload(),
            qos: BAMBU_MQTT_QOS,
        })
        .await
        .with_context(|| format!("publish get_version to request topic {}", topics.request))?;

    tokio::time::timeout(report_timeout, async {
        loop {
            let report = transport
                .next_report(report_timeout)
                .await
                .context("wait for MQTT get_version report")?;
            if let Some(observation) = parse_firmware_version_observation(&report)? {
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
