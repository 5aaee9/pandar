use std::time::Duration;

use anyhow::Context;
use pandar_core::created_at_now;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{
    AgentConfig,
    machine::{
        BambuPrinterEndpoint, MachineSnapshot, MaterialRefreshResult,
        materials::normalize_material_patch,
    },
    protocol::agent::v1::{
        AgentEvent, MachineDiagnostic, NozzleTemperature, PrintJobReport, PrinterMaterialsSnapshot,
        PrinterSnapshot, agent_event,
    },
};

use super::{
    BambuMqttTopics, BambuMqttTransport, MachineReportDiagnostic, PrintReportProgress,
    snapshot_from_report,
};

pub fn print_report_from_report(
    endpoint: &BambuPrinterEndpoint,
    report: &Value,
) -> PrintReportProgress {
    let print = report.get("print").unwrap_or(&Value::Null);
    let subtask_id = trimmed_string(print.get("subtask_id"));

    let mut diagnostics = Vec::new();
    if let Some(print_error) = print.get("print_error").and_then(diagnostic_message) {
        diagnostics.push(MachineReportDiagnostic {
            kind: "print_error".to_owned(),
            severity: "error".to_owned(),
            code: None,
            message: print_error,
            payload: print.get("print_error").cloned().unwrap_or(Value::Null),
        });
    }
    collect_hms_diagnostics(report, &mut diagnostics);

    let observed_at = created_at_now();
    let printer_materials_json = normalize_material_patch(report, &observed_at)
        .and_then(|patch| serde_json::to_string(&patch).ok())
        .unwrap_or_default();

    PrintReportProgress {
        serial: endpoint.serial.clone(),
        job_id: trimmed_string(print.get("task_id")),
        artifact_id: subtask_id.clone(),
        subtask_id,
        gcode_state: trimmed_string(print.get("gcode_state")),
        percent: bounded_u32(print.get("mc_percent"), 0, 100).map(|value| value as u8),
        remaining_time_minutes: bounded_u32(print.get("mc_remaining_time"), 0, 4320),
        current_layer: bounded_u32(print.get("layer_num"), 0, 100_000),
        total_layers: bounded_u32(print.get("total_layer_num"), 0, 100_000),
        gcode_file: trimmed_string(print.get("gcode_file")),
        subtask_name: trimmed_string(print.get("subtask_name")),
        diagnostics,
        observed_at,
        printer_materials_json,
    }
}

pub fn print_job_report_event(config: &AgentConfig, progress: PrintReportProgress) -> AgentEvent {
    AgentEvent {
        agent_id: config.agent_id.clone(),
        tenant_id: config.tenant_id.clone(),
        event_id: format!("print-report-{}", progress.serial),
        event: Some(agent_event::Event::PrintJobReport(PrintJobReport {
            serial: progress.serial,
            job_id: progress.job_id.unwrap_or_default(),
            artifact_id: progress.artifact_id.unwrap_or_default(),
            subtask_id: progress.subtask_id.unwrap_or_default(),
            gcode_file: progress.gcode_file.unwrap_or_default(),
            subtask_name: progress.subtask_name.unwrap_or_default(),
            gcode_state: progress.gcode_state.unwrap_or_default(),
            percent: progress.percent.unwrap_or_default().into(),
            has_percent: progress.percent.is_some(),
            remaining_time_minutes: progress.remaining_time_minutes.unwrap_or_default(),
            has_remaining_time_minutes: progress.remaining_time_minutes.is_some(),
            current_layer: progress.current_layer.unwrap_or_default(),
            has_current_layer: progress.current_layer.is_some(),
            total_layers: progress.total_layers.unwrap_or_default(),
            has_total_layers: progress.total_layers.is_some(),
            diagnostics: progress
                .diagnostics
                .into_iter()
                .map(|diagnostic| MachineDiagnostic {
                    kind: diagnostic.kind,
                    severity: diagnostic.severity,
                    code: diagnostic.code.unwrap_or_default(),
                    message: diagnostic.message,
                    payload_json: serde_json::to_string(&diagnostic.payload)
                        .unwrap_or_else(|_| "null".to_owned()),
                })
                .collect(),
            observed_at: progress.observed_at,
            printer_materials_json: progress.printer_materials_json,
        })),
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
                })
                .collect(),
            bed_temperature_celsius: snapshot.bed_temperature_celsius.unwrap_or_default(),
            bed_target_temperature_celsius: snapshot
                .bed_target_temperature_celsius
                .unwrap_or_default(),
            chamber_temperature_celsius: snapshot.chamber_temperature_celsius.unwrap_or_default(),
            active_nozzle: snapshot.active_nozzle.unwrap_or_default(),
        })),
    }
}

fn snapshot_has_temperature_telemetry(snapshot: &MachineSnapshot) -> bool {
    !snapshot.nozzle_temperatures.is_empty()
        || snapshot.bed_temperature_celsius.is_some()
        || snapshot.bed_target_temperature_celsius.is_some()
        || snapshot.chamber_temperature_celsius.is_some()
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
                let progress = print_report_from_report(endpoint, &report);
                let snapshot = snapshot_from_report(endpoint, &report);
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

pub(super) fn trimmed_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn bounded_u32(value: Option<&Value>, min: u32, max: u32) -> Option<u32> {
    let value = match value? {
        Value::Number(number) => {
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
        Value::String(raw) => raw.trim().parse().ok()?,
        _ => return None,
    };

    (min..=max).contains(&value).then_some(value)
}

fn diagnostic_message(value: &Value) -> Option<String> {
    match value {
        Value::String(raw) => {
            let trimmed = raw.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        }
        Value::Object(_) => message_from_object(value),
        Value::Null => None,
        other => Some(other.to_string()).filter(|message| !message.is_empty()),
    }
}

fn collect_hms_diagnostics(report: &Value, diagnostics: &mut Vec<MachineReportDiagnostic>) {
    for container in [report, report.get("print").unwrap_or(&Value::Null)] {
        let Value::Object(fields) = container else {
            continue;
        };
        for (key, value) in fields {
            if key.to_ascii_lowercase().contains("hms") {
                collect_hms_value(value, diagnostics);
            }
        }
    }
}

fn collect_hms_value(value: &Value, diagnostics: &mut Vec<MachineReportDiagnostic>) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_hms_value(value, diagnostics);
            }
        }
        Value::Object(_) => {
            if let (Some(code), Some(message)) =
                (code_from_object(value), message_from_object(value))
            {
                diagnostics.push(MachineReportDiagnostic {
                    kind: "hms".to_owned(),
                    severity: "warning".to_owned(),
                    code: Some(code),
                    message,
                    payload: value.clone(),
                });
            }
        }
        _ => {}
    }
}

fn code_from_object(value: &Value) -> Option<String> {
    ["code", "hms_code", "error_code"]
        .into_iter()
        .find_map(|key| trimmed_string(value.get(key)))
}

fn message_from_object(value: &Value) -> Option<String> {
    ["message", "msg", "description", "info"]
        .into_iter()
        .find_map(|key| trimmed_string(value.get(key)))
}
