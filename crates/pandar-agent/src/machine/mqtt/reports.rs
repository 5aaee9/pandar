mod protocol;
mod schema;

pub use protocol::print_job_report_event;

use std::{collections::BTreeMap, time::Duration};

use anyhow::Context;
use pandar_core::created_at_now;
use schema::{HmsValue, NumericValue, PrintReportEnvelope};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{
    AgentConfig,
    machine::{
        BambuPrinterEndpoint, MachineSnapshot, MaterialRefreshResult,
        materials::{normalize_material_patch, parse_materials_report},
        types::decode_json_payload,
    },
    protocol::agent::v1::{
        AgentEvent, NozzleTemperature, PrinterMaterialsSnapshot, PrinterSnapshot, agent_event,
    },
};

use super::{
    BambuMqttTopics, BambuMqttTransport, MachineReportDiagnostic, MachineReportDiagnosticPayload,
    PrintReportProgress, parse_snapshot_report, snapshot_from_parsed_report,
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
) -> anyhow::Result<()>
where
    T: BambuMqttTransport + ?Sized,
{
    let topics = BambuMqttTopics::for_serial(&endpoint.serial);
    transport
        .subscribe(&topics.report)
        .await
        .with_context(|| format!("subscribe to report topic {}", topics.report))?;

    loop {
        if sender.is_closed() {
            break;
        }

        match transport.next_report(report_timeout).await {
            Ok(report) => {
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
                let snapshot = snapshot_from_parsed_report(endpoint, snapshot_report.as_ref());
                let snapshot_event = snapshot_has_temperature_telemetry(&snapshot)
                    .then(|| printer_snapshot_event(config, snapshot));
                let materials =
                    (!progress.printer_materials_json.is_empty()).then(|| MaterialRefreshResult {
                        serial: progress.serial.clone(),
                        printer_id: None,
                        printer_materials_json: progress.printer_materials_json.clone(),
                    });
                if sender
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
                if let Some(materials) = materials
                    && sender
                        .send(printer_materials_snapshot_event(config, materials))
                        .await
                        .is_err()
                {
                    break;
                }
            }
            Err(err) => {
                tracing::warn!(
                    serial = %endpoint.serial,
                    error = %format!("{err:#}"),
                    "printer report receive failed"
                );
            }
        }
    }

    Ok(())
}

fn raw_print_payload(report: &Value) -> Option<MachineReportDiagnosticPayload> {
    report.get("print").map(value_payload)
}

fn value_payload(value: &Value) -> MachineReportDiagnosticPayload {
    match value {
        Value::Object(object) => MachineReportDiagnosticPayload::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), value_payload(value)))
                .collect(),
        ),
        Value::Array(values) => {
            MachineReportDiagnosticPayload::Array(values.iter().map(value_payload).collect())
        }
        Value::String(value) => MachineReportDiagnosticPayload::String(value.clone()),
        Value::Number(value) => MachineReportDiagnosticPayload::Number(value.clone()),
        Value::Bool(value) => MachineReportDiagnosticPayload::Bool(*value),
        Value::Null => MachineReportDiagnosticPayload::Null,
    }
}

fn print_error_payload(
    print_error: MachineReportDiagnosticPayload,
    raw_print: Option<MachineReportDiagnosticPayload>,
) -> MachineReportDiagnosticPayload {
    let Some(raw_print) = raw_print else {
        return print_error;
    };

    let mut fields = BTreeMap::new();
    fields.insert("print_error".to_owned(), print_error);
    fields.insert("raw_print".to_owned(), raw_print);
    MachineReportDiagnosticPayload::Object(fields)
}

pub(super) fn trimmed_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn bounded_u32(value: Option<&NumericValue>, min: u32, max: u32) -> Option<u32> {
    let value = match value? {
        NumericValue::Number(number) => {
            if let Some(value) = number.as_u64() {
                u32::try_from(value).ok()?
            } else if let Some(value) = number.as_i64() {
                u32::try_from(value).ok()?
            } else {
                let value = number.as_f64()?;
                if !value.is_finite() || value.fract() != 0.0 || value < 0.0 {
                    return None;
                }
                u32::try_from(value as u64).ok()?
            }
        }
        NumericValue::String(raw) => raw.trim().parse().ok()?,
    };

    (min..=max).contains(&value).then_some(value)
}

fn collect_hms_diagnostics(
    envelope: &PrintReportEnvelope,
    diagnostics: &mut Vec<MachineReportDiagnostic>,
) {
    if let Some(hms) = &envelope.print.hms {
        diagnostics.extend(hms.iter().filter_map(|item| item.diagnostic()));
    }

    for fields in [&envelope.fields, &envelope.print.fields] {
        for value in hms_values(fields) {
            let mut objects = Vec::new();
            value.collect_objects(&mut objects);
            diagnostics.extend(objects.into_iter().filter_map(|object| object.diagnostic()));
        }
    }
}

fn hms_values(
    fields: &std::collections::BTreeMap<String, HmsValue>,
) -> impl Iterator<Item = &HmsValue> {
    fields
        .iter()
        .filter(|(key, _)| key.to_ascii_lowercase().contains("hms"))
        .map(|(_, value)| value)
}
