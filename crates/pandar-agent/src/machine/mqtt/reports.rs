mod diagnostics;
mod firmware;
mod forwarding;
mod protocol;

use super::report::print::{PrintReportEnvelope, ReportJson};
pub(crate) use forwarding::{
    MqttForwardingContext, MqttPresenceState, forward_print_reports_with_context,
};
#[cfg(test)]
pub use forwarding::{forward_print_reports, forward_print_reports_with_firmware};
#[cfg(test)]
use pandar_core::created_at_now;
pub use protocol::print_job_report_event;

use crate::{
    AgentConfig,
    machine::{BambuPrinterEndpoint, MachineSnapshot, MaterialRefreshResult},
    protocol::agent::v1::{
        AgentEvent, NozzleTemperature, PrinterMaterialsSnapshot, PrinterSnapshot, agent_event,
    },
};

#[cfg(test)]
use crate::machine::materials::normalize_material_patch;

#[cfg(test)]
use super::MachineReport;
use super::{MachineReportDiagnostic, MachineReportDiagnosticPayload, PrintReportProgress};
use diagnostics::{bounded_u32, collect_hms_diagnostics, print_error_payload, trimmed_string};

#[cfg(test)]
pub(crate) fn print_report_from_report(
    endpoint: &BambuPrinterEndpoint,
    report: &MachineReport,
) -> PrintReportProgress {
    let observed_at = created_at_now();
    let printer_materials_json = report
        .materials()
        .and_then(|report| normalize_material_patch(report, &observed_at))
        .and_then(|patch| serde_json::to_string(&patch).ok())
        .unwrap_or_default();

    print_report_from_parsed_report(
        endpoint,
        report.print(),
        report.raw_print_payload(),
        observed_at,
        printer_materials_json,
    )
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
        gcode_state: trimmed_string(print.gcode_state.as_deref()).or_else(|| match &print.state {
            Some(ReportJson::String(state)) => trimmed_string(Some(state)),
            _ => None,
        }),
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
            state: snapshot.state.unwrap_or_default(),
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
                    snow: temperature.snow,
                    hnow: temperature.hnow,
                })
                .collect(),
            bed_temperature_celsius: snapshot.bed_temperature_celsius.unwrap_or_default(),
            bed_target_temperature_celsius: snapshot
                .bed_target_temperature_celsius
                .unwrap_or_default(),
            chamber_temperature_celsius: snapshot.chamber_temperature_celsius.unwrap_or_default(),
            chamber_target_temperature_celsius: snapshot
                .chamber_target_temperature_celsius
                .unwrap_or_default(),
            active_nozzle: snapshot.active_nozzle.unwrap_or_default(),
            chamber_light_on: snapshot.chamber_light_on,
            device_features: crate::protocol::proto_device_features(
                snapshot.device_features,
                snapshot.device_features2,
            ),
            connection_authoritative: false,
            telemetry_authoritative: snapshot.telemetry_authoritative,
            nozzle_system: snapshot
                .nozzle_system
                .map(crate::protocol::proto_nozzle_system),
        })),
    }
}
