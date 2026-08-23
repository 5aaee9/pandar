use pandar_core::{AgentId, JobId, TenantId};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tonic::Status;

use crate::{
    AppState,
    grpc::commands::repository_status,
    metrics::PrintReportMetric,
    printer_events::{PrinterEvent, fence_printer_nozzle_system, printer_event_printer},
    protocol::agent::v1::PrintJobReport,
    repositories::{ApplyPrintReport, MaterialPatchOutcome, PrintReportDiagnostic, PrinterHms},
    routes::jobs::JobResponse,
    sessions::SessionToken,
};

pub async fn handle_print_report(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    report: PrintJobReport,
) -> Result<(), Status> {
    let input = match apply_input(tenant_id, agent_id, report) {
        Ok(input) => input,
        Err(err) => {
            state
                .metrics()
                .record_print_report(PrintReportMetric::Rejected);
            return Err(err);
        }
    };
    let report_serial = input.serial.clone();
    let has_material_patch = !input.printer_materials_json.trim().is_empty();
    let _lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    if !state.sessions().is_current(agent_id, token).await {
        return Ok(());
    }
    let applied = match state
        .jobs()
        .apply_current_print_report(&token.persisted_id(), input)
        .await
    {
        Ok(applied) => {
            state
                .metrics()
                .record_print_report(PrintReportMetric::Accepted);
            applied
        }
        Err(crate::repositories::RepositoryError::AgentSessionNotCurrent) => return Ok(()),
        Err(err) => {
            state
                .metrics()
                .record_print_report(PrintReportMetric::Rejected);
            return Err(repository_status(err));
        }
    };
    if let Some(job) = applied.job
        && (applied.changed || applied.inserted_job_events)
    {
        state
            .publish_printer_event(
                tenant_id,
                PrinterEvent::JobProgress {
                    job: Box::new(JobResponse::try_from(job).map_err(repository_status)?),
                },
            )
            .await;
    }
    let material_changed = matches!(&applied.material_outcome, MaterialPatchOutcome::Changed(_));
    match &applied.material_outcome {
        MaterialPatchOutcome::Changed(_) => {}
        MaterialPatchOutcome::Invalid { error } => {
            tracing::warn!(
                %tenant_id,
                %agent_id,
                serial = %report_serial,
                reason = "invalid_patch",
                error = %crate::redaction::redact_secrets(error),
                "ignored print report material patch"
            );
        }
        MaterialPatchOutcome::Older if has_material_patch => {
            tracing::info!(
                %tenant_id,
                %agent_id,
                serial = %report_serial,
                reason = "older_patch",
                "ignored print report material patch"
            );
        }
        MaterialPatchOutcome::Unchanged(_) if has_material_patch => {
            tracing::info!(
                %tenant_id,
                %agent_id,
                serial = %report_serial,
                reason = "unchanged_patch",
                "ignored print report material patch"
            );
        }
        MaterialPatchOutcome::Empty
        | MaterialPatchOutcome::Older
        | MaterialPatchOutcome::Unchanged(_) => {}
    }
    if (applied.live_status_changed || material_changed)
        && let Some(printer) = applied.printer
    {
        let printer_id = printer.printer.id.clone();
        let serial_number = printer.printer.serial_number.clone();
        let materials = state
            .materials()
            .latest_for_printer(tenant_id, &printer.printer.id)
            .await
            .map_err(repository_status)?;
        let printer = fence_printer_nozzle_system(state.sessions(), tenant_id, printer).await;
        state
            .publish_printer_event(
                tenant_id,
                PrinterEvent::PrinterSnapshot {
                    printer: Box::new(printer_event_printer(printer, materials)),
                },
            )
            .await;
        state
            .publish_printer_projection_change(tenant_id, &printer_id, &serial_number)
            .await;
    }
    Ok(())
}

fn apply_input(
    tenant_id: TenantId,
    agent_id: AgentId,
    report: PrintJobReport,
) -> Result<ApplyPrintReport, Status> {
    let serial = required(&report.serial, "serial must not be blank")?;
    let observed_at = required(&report.observed_at, "observed_at must not be blank")?;
    validate_rfc3339(&observed_at)?;
    let task_id = trim_optional(report.job_id.clone());
    let job_id = optional_job_id(&report.job_id)?;
    let artifact_id = optional_uuid_string(&report.artifact_id, "artifact_id must be a UUID")?;
    let diagnostics = report
        .diagnostics
        .into_iter()
        .filter_map(diagnostic)
        .collect();

    Ok(ApplyPrintReport {
        tenant_id,
        agent_id,
        serial,
        task_id,
        job_id,
        print_error: report
            .has_print_error
            .then_some(report.print_error)
            .filter(|value| *value <= i32::MAX as u32),
        printer_job_id: report.has_printer_job_id.then_some(report.printer_job_id),
        job_attr: report.has_job_attr.then_some(report.job_attr),
        artifact_id,
        subtask_id: trim_optional(report.subtask_id),
        gcode_file: trim_optional(report.gcode_file),
        subtask_name: trim_optional(report.subtask_name),
        gcode_state: trim_optional(report.gcode_state),
        percent: report
            .has_percent
            .then_some(report.percent)
            .filter(|value| *value <= 100)
            .map(|value| value as u8),
        speed_level: report
            .has_speed_level
            .then_some(report.speed_level)
            .filter(|value| (1..=4).contains(value))
            .map(|value| value as u8),
        remaining_time_minutes: report
            .has_remaining_time_minutes
            .then_some(report.remaining_time_minutes)
            .filter(|value| *value <= 4320),
        current_layer: report
            .has_current_layer
            .then_some(report.current_layer)
            .filter(|value| *value <= 100_000),
        total_layers: report
            .has_total_layers
            .then_some(report.total_layers)
            .filter(|value| *value <= 100_000),
        hms: report.has_hms.then(|| {
            report
                .hms
                .into_iter()
                .map(|item| PrinterHms {
                    attr: item.attr,
                    code: item.code,
                })
                .collect()
        }),
        diagnostics,
        printer_materials_json: report.printer_materials_json,
        observed_at,
    })
}

fn required(value: &str, message: &'static str) -> Result<String, Status> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Status::invalid_argument(message));
    }

    Ok(value.to_string())
}

fn optional_job_id(value: &str) -> Result<Option<JobId>, Status> {
    let Some(value) = trim_optional(value.to_string()) else {
        return Ok(None);
    };
    match JobId::parse(&value) {
        Ok(job_id) => Ok(Some(job_id)),
        Err(_) => {
            tracing::debug!(job_id = %value, "ignoring non-Pandar printer report job_id");
            Ok(None)
        }
    }
}

fn optional_uuid_string(value: &str, message: &'static str) -> Result<Option<String>, Status> {
    let Some(value) = trim_optional(value.to_string()) else {
        return Ok(None);
    };
    match uuid::Uuid::parse_str(&value) {
        Ok(_) => Ok(Some(value)),
        Err(_) => {
            tracing::debug!(value = %value, %message, "ignoring non-Pandar print report identifier");
            Ok(None)
        }
    }
}

fn trim_optional(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn diagnostic(
    diagnostic: crate::protocol::agent::v1::MachineDiagnostic,
) -> Option<PrintReportDiagnostic> {
    let kind = trim_optional(diagnostic.kind)?;
    let message = trim_optional(diagnostic.message).unwrap_or_default();
    let payload_json = trim_optional(diagnostic.payload_json).unwrap_or_else(|| "{}".to_string());
    Some(PrintReportDiagnostic {
        kind,
        severity: normalized_severity(trim_optional(diagnostic.severity)),
        code: trim_optional(diagnostic.code),
        message,
        payload_json,
    })
}

fn normalized_severity(severity: Option<String>) -> String {
    match severity.as_deref() {
        None | Some("info") => "info".to_string(),
        Some("warning") => "warning".to_string(),
        Some("error") => "error".to_string(),
        Some(_) => "warning".to_string(),
    }
}

fn validate_rfc3339(value: &str) -> Result<(), Status> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|_| ())
        .map_err(|_| Status::invalid_argument("observed_at must be RFC3339"))
}
