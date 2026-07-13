mod diagnostics;
mod firmware;
mod protocol;
mod schema;

pub use protocol::print_job_report_event;

use std::time::Duration;

use anyhow::Context;
use pandar_core::created_at_now;
use schema::PrintReportEnvelope;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{
    AgentConfig,
    machine::{
        BambuPrinterEndpoint, DeviceFeatureCache, FirmwareReportContext, MachineSnapshot,
        MaterialRefreshResult,
        materials::{normalize_material_patch, parse_materials_report},
        types::decode_json_payload,
    },
    protocol::agent::v1::{
        AgentEvent, NozzleTemperature, PrinterDeviceFeatures, PrinterMaterialsSnapshot,
        PrinterSnapshot, agent_event,
    },
};

use super::device_features::is_feature_only_report;
use super::{
    BAMBU_MQTT_QOS, BambuMqttCommand, BambuMqttTopics, BambuMqttTransport, MachineReportDiagnostic,
    MachineReportDiagnosticPayload, PrintReportProgress, PublishedMqttCommand,
    device_feature_observation, feature_event, is_mqtt_report_idle_timeout, parse_snapshot_report,
    snapshot_from_parsed_report,
};
use diagnostics::{
    bounded_u32, collect_hms_diagnostics, print_error_payload, raw_print_payload, trimmed_string,
};

pub fn print_report_from_report(
    endpoint: &BambuPrinterEndpoint,
    report: &Value,
) -> PrintReportProgress {
    let envelope = parse_print_report(report);
    let observed_at = created_at_now();
    let materials_report = parse_materials_report(report);
    let printer_materials_json = materials_report
        .as_ref()
        .and_then(|report| normalize_material_patch(report, &observed_at))
        .and_then(|patch| serde_json::to_string(&patch).ok())
        .unwrap_or_default();

    print_report_from_parsed_report(
        endpoint,
        envelope.as_ref(),
        raw_print_payload(report),
        observed_at,
        printer_materials_json,
    )
}

pub(crate) fn parse_print_report(report: &Value) -> Option<PrintReportEnvelope> {
    decode_json_payload(report)
}

pub(crate) fn print_report_from_parsed_report(
    endpoint: &BambuPrinterEndpoint,
    envelope: Option<&PrintReportEnvelope>,
    raw_print: Option<MachineReportDiagnosticPayload>,
    observed_at: String,
    printer_materials_json: String,
) -> PrintReportProgress {
    let default_envelope = PrintReportEnvelope::default();
    let envelope = envelope.unwrap_or(&default_envelope);
    let print = &envelope.print;
    let subtask_id = trimmed_string(print.subtask_id.as_deref());

    let mut diagnostics = Vec::new();
    if let Some(print_error) = print
        .print_error
        .as_ref()
        .and_then(|value| value.diagnostic())
        && let Some(message) = print_error.message()
    {
        diagnostics.push(MachineReportDiagnostic {
            kind: "print_error".to_owned(),
            severity: "error".to_owned(),
            code: None,
            message,
            payload: print_error_payload(print_error.payload(), raw_print.clone()),
        });
    }
    collect_hms_diagnostics(envelope, &mut diagnostics);

    PrintReportProgress {
        serial: endpoint.serial.clone(),
        job_id: trimmed_string(print.task_id.as_deref()),
        job_attr: bounded_u32(print.job_attr.as_ref(), 0, u32::MAX),
        print_error: print.print_error.as_ref().and_then(|value| value.state()),
        printer_job_id: print.job_id.clone(),
        artifact_id: subtask_id.clone(),
        subtask_id,
        gcode_state: trimmed_string(print.gcode_state.as_deref()),
        percent: bounded_u32(print.mc_percent.as_ref(), 0, 100).map(|value| value as u8),
        remaining_time_minutes: bounded_u32(print.mc_remaining_time.as_ref(), 0, 4320),
        current_layer: bounded_u32(print.layer_num.as_ref(), 0, 100_000),
        total_layers: bounded_u32(print.total_layer_num.as_ref(), 0, 100_000),
        gcode_file: trimmed_string(print.gcode_file.as_deref()),
        subtask_name: trimmed_string(print.subtask_name.as_deref()),
        hms: print
            .hms
            .as_ref()
            .and_then(|items| items.iter().map(|item| item.machine()).collect()),
        diagnostics,
        observed_at,
        printer_materials_json,
    }
}

pub fn printer_materials_snapshot_event(
    config: &AgentConfig,
    materials: MaterialRefreshResult,
) -> AgentEvent {
    AgentEvent {
        agent_id: config.agent_id.clone(),
        tenant_id: config.tenant_id.clone(),
        event_id: format!("printer-materials-{}", materials.serial),
        event: Some(agent_event::Event::PrinterMaterialsSnapshot(
            PrinterMaterialsSnapshot {
                serial: materials.serial,
                printer_id: materials.printer_id.unwrap_or_default(),
                printer_materials_json: materials.printer_materials_json,
            },
        )),
    }
}

fn printer_snapshot_event(config: &AgentConfig, snapshot: MachineSnapshot) -> AgentEvent {
    AgentEvent {
        agent_id: config.agent_id.clone(),
        tenant_id: config.tenant_id.clone(),
        event_id: format!("printer-snapshot-{}", snapshot.serial),
        event: Some(agent_event::Event::PrinterSnapshot(PrinterSnapshot {
            serial: snapshot.serial,
            host: snapshot.host.unwrap_or_default(),
            access_code: snapshot.access_code.unwrap_or_default(),
            name: snapshot.name,
            state: snapshot.state,
            model: snapshot.model.unwrap_or_default(),
            nozzle_temperatures: snapshot
                .nozzle_temperatures
                .into_iter()
                .map(|temperature| NozzleTemperature {
                    label: temperature.label.unwrap_or_default(),
                    current_celsius: temperature.current_celsius.unwrap_or_default(),
                    target_celsius: temperature.target_celsius.unwrap_or_default(),
                    diameter_mm: temperature.diameter_mm.unwrap_or_default(),
                    nozzle_type: temperature.nozzle_type.unwrap_or_default(),
                })
                .collect(),
            bed_temperature_celsius: snapshot.bed_temperature_celsius.unwrap_or_default(),
            bed_target_temperature_celsius: snapshot
                .bed_target_temperature_celsius
                .unwrap_or_default(),
            chamber_temperature_celsius: snapshot.chamber_temperature_celsius.unwrap_or_default(),
            active_nozzle: snapshot.active_nozzle.unwrap_or_default(),
            chamber_light_on: snapshot.chamber_light_on,
            device_features: snapshot
                .device_features
                .map(|features| PrinterDeviceFeatures {
                    bambu_fun_bits: features.bits(),
                }),
        })),
    }
}

fn snapshot_has_temperature_telemetry(snapshot: &MachineSnapshot) -> bool {
    !snapshot.nozzle_temperatures.is_empty()
        || snapshot.bed_temperature_celsius.is_some()
        || snapshot.bed_target_temperature_celsius.is_some()
        || snapshot.chamber_temperature_celsius.is_some()
        || snapshot.chamber_light_on.is_some()
}

pub async fn forward_print_reports<T>(
    config: &AgentConfig,
    transport: &T,
    endpoint: &BambuPrinterEndpoint,
    report_timeout: Duration,
    sender: &mpsc::Sender<AgentEvent>,
    cache: &DeviceFeatureCache,
) -> anyhow::Result<()>
where
    T: BambuMqttTransport + ?Sized,
{
    forward_print_reports_inner(
        config,
        transport,
        endpoint,
        report_timeout,
        sender,
        cache,
        None,
    )
    .await
}

pub async fn forward_print_reports_with_firmware<T>(
    config: &AgentConfig,
    transport: &T,
    endpoint: &BambuPrinterEndpoint,
    report_timeout: Duration,
    sender: &mpsc::Sender<AgentEvent>,
    cache: &DeviceFeatureCache,
    firmware: FirmwareReportContext,
) -> anyhow::Result<()>
where
    T: BambuMqttTransport + ?Sized,
{
    forward_print_reports_inner(
        config,
        transport,
        endpoint,
        report_timeout,
        sender,
        cache,
        Some(firmware),
    )
    .await
}

async fn forward_print_reports_inner<T>(
    config: &AgentConfig,
    transport: &T,
    endpoint: &BambuPrinterEndpoint,
    report_timeout: Duration,
    sender: &mpsc::Sender<AgentEvent>,
    cache: &DeviceFeatureCache,
    firmware_context: Option<FirmwareReportContext>,
) -> anyhow::Result<()>
where
    T: BambuMqttTransport + ?Sized,
{
    let topics = BambuMqttTopics::for_serial(&endpoint.serial);
    transport
        .subscribe(&topics.report)
        .await
        .with_context(|| format!("subscribe to report topic {}", topics.report))?;
    let mut firmware_processor = if let Some(context) = firmware_context {
        Some(
            firmware::FirmwareReportProcessor::start(
                endpoint,
                context,
                report_timeout,
                transport,
                &topics,
            )
            .await?,
        )
    } else {
        None
    };
    transport
        .publish(PublishedMqttCommand {
            topic: topics.request.clone(),
            payload: BambuMqttCommand::RequestPushAll.payload(),
            qos: BAMBU_MQTT_QOS,
        })
        .await
        .with_context(|| format!("publish pushall to request topic {}", topics.request))?;

    loop {
        if sender.is_closed() {
            break;
        }

        match transport.next_report(report_timeout).await {
            Ok(report) => {
                if let Some(processor) = &mut firmware_processor {
                    processor.observe(config, &report, sender).await?;
                }
                let observed_at = created_at_now();
                let print_report = parse_print_report(&report);
                let materials_report = parse_materials_report(&report);
                let printer_materials_json = materials_report
                    .as_ref()
                    .and_then(|report| normalize_material_patch(report, &observed_at))
                    .and_then(|patch| serde_json::to_string(&patch).ok())
                    .unwrap_or_default();
                let progress = print_report_from_parsed_report(
                    endpoint,
                    print_report.as_ref(),
                    raw_print_payload(&report),
                    observed_at,
                    printer_materials_json,
                );
                let snapshot_report = parse_snapshot_report(&report);
                let device_features = match snapshot_report
                    .as_ref()
                    .map(|report| device_feature_observation(&endpoint.serial, report))
                    .transpose()
                {
                    Ok(value) => value.flatten(),
                    Err(error) => {
                        tracing::warn!(
                            serial = %endpoint.serial,
                            error = %format!("{error:#}"),
                            "invalid printer device feature observation"
                        );
                        None
                    }
                };
                if let Some(value) = device_features {
                    cache.update(&endpoint.serial, value).await;
                }
                let mut snapshot = snapshot_from_parsed_report(endpoint, snapshot_report.as_ref());
                snapshot.device_features = device_features;
                let snapshot_event = snapshot_has_temperature_telemetry(&snapshot)
                    .then(|| printer_snapshot_event(config, snapshot));
                let feature_event = (snapshot_event.is_none() && device_features.is_some())
                    .then(|| feature_event(config, endpoint.serial.clone(), device_features));
                let materials =
                    (!progress.printer_materials_json.is_empty()).then(|| MaterialRefreshResult {
                        serial: progress.serial.clone(),
                        printer_id: None,
                        printer_materials_json: progress.printer_materials_json.clone(),
                    });
                if super::has_non_firmware_print_telemetry(&report)
                    && !is_feature_only_report(&report)
                    && sender
                        .send(print_job_report_event(config, progress))
                        .await
                        .is_err()
                {
                    break;
                }
                if let Some(snapshot_event) = snapshot_event
                    && sender.send(snapshot_event).await.is_err()
                {
                    break;
                }
                if let Some(feature_event) = feature_event
                    && sender.send(feature_event).await.is_err()
                {
                    break;
                }
                if let Some(materials) = materials
                    && sender
                        .send(printer_materials_snapshot_event(config, materials))
                        .await
                        .is_err()
                {
                    break;
                }
            }
            Err(err) if is_mqtt_report_idle_timeout(&err) => {
                if let Some(processor) = &mut firmware_processor {
                    processor.expire_version_observation();
                }
                tracing::warn!(
                    serial = %endpoint.serial,
                    error = %format!("{err:#}"),
                    "printer report receive failed"
                );
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!(
                        "receive printer {} report from topic {}",
                        endpoint.serial, topics.report
                    )
                });
            }
        }
    }

    Ok(())
}
