use crate::{
    AgentConfig,
    protocol::agent::v1::{
        AgentEvent, MachineDiagnostic, PrintJobReport, PrinterHmsItem, agent_event,
    },
};

use super::super::PrintReportProgress;

pub fn print_job_report_event(config: &AgentConfig, progress: PrintReportProgress) -> AgentEvent {
    let has_hms = progress.hms.is_some();
    let hms = progress
        .hms
        .unwrap_or_default()
        .into_iter()
        .map(|item| PrinterHmsItem {
            attr: item.attr,
            code: item.code,
        })
        .collect();
    let has_print_error = progress.print_error.is_some();
    let print_error = progress.print_error.unwrap_or_default();
    let has_printer_job_id = progress.printer_job_id.is_some();
    let printer_job_id = progress.printer_job_id.unwrap_or_default();

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
            hms,
            has_hms,
            print_error,
            has_print_error,
            printer_job_id,
            has_printer_job_id,
        })),
    }
}
