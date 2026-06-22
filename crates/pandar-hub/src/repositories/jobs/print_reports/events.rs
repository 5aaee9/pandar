use anyhow::Context;
use pandar_core::PrintStatus;
use sea_orm::{ActiveValue::Set, EntityTrait, TryInsertResult};
use sha2::{Digest, Sha256};

use crate::{
    entities::machine_events,
    repositories::{JobWithArtifact, RepositoryResult},
};

use super::{
    ApplyPrintReport, PrintReportDiagnostic,
    correlation::PrinterMatch,
    state::{PrintUpdate, is_terminal_status},
};

#[derive(Debug, Clone)]
struct MachineEventInsert {
    event_key: String,
    kind: String,
    severity: String,
    message: String,
    code: Option<String>,
    payload_json: String,
}

pub(super) async fn insert_job_events<C>(
    connection: &C,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
    job: &JobWithArtifact,
    update: &PrintUpdate,
) -> RepositoryResult<bool>
where
    C: sea_orm::ConnectionTrait,
{
    let mut inserted = false;
    if progress_event_key(job, input).is_some() {
        inserted |= insert_event(
            connection,
            input,
            printer,
            Some(job),
            progress_event(input, job)?,
        )
        .await?;
    }
    if let Some(event) = terminal_event(input, job, update) {
        inserted |= insert_event(connection, input, printer, Some(job), event).await?;
    }
    for event in diagnostic_events(input, printer, Some(job)) {
        inserted |= insert_event(connection, input, printer, Some(job), event).await?;
    }
    Ok(inserted)
}

pub(super) async fn insert_printer_events<C>(
    connection: &C,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
) -> RepositoryResult<bool>
where
    C: sea_orm::ConnectionTrait,
{
    let mut inserted = false;
    for event in diagnostic_events(input, printer, None) {
        inserted |= insert_event(connection, input, printer, None, event).await?;
    }
    Ok(inserted)
}

async fn insert_event<C>(
    connection: &C,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
    job: Option<&JobWithArtifact>,
    event: MachineEventInsert,
) -> RepositoryResult<bool>
where
    C: sea_orm::ConnectionTrait,
{
    let result = machine_events::Entity::insert(machine_events::ActiveModel {
        id: Set(uuid::Uuid::new_v4().to_string()),
        tenant_id: Set(input.tenant_id.to_string()),
        agent_id: Set(input.agent_id.to_string()),
        printer_id: Set(printer.id.clone()),
        job_id: Set(job.map(|job| job.job.id.to_string())),
        event_key: Set(event.event_key),
        kind: Set(event.kind),
        severity: Set(event.severity),
        message: Set(event.message),
        code: Set(event.code),
        payload_json: Set(event.payload_json),
        observed_at: Set(input.observed_at.clone()),
        created_at: Set(pandar_core::created_at_now()),
    })
    .on_conflict_do_nothing_on([
        machine_events::Column::TenantId,
        machine_events::Column::EventKey,
    ])
    .exec_without_returning(connection)
    .await
    .context("failed to insert machine event")?;

    Ok(matches!(result, TryInsertResult::Inserted(rows) if rows > 0))
}

fn progress_event(
    input: &ApplyPrintReport,
    job: &JobWithArtifact,
) -> RepositoryResult<MachineEventInsert> {
    let event_key = progress_event_key(job, input).context("missing progress event key")?;
    Ok(MachineEventInsert {
        event_key,
        kind: "print_progress".to_string(),
        severity: "info".to_string(),
        message: progress_message(input),
        code: None,
        payload_json: progress_payload(input),
    })
}

fn progress_event_key(job: &JobWithArtifact, input: &ApplyPrintReport) -> Option<String> {
    let has_progress = input.gcode_state.is_some()
        || input.percent.is_some()
        || input.current_layer.is_some()
        || input.total_layers.is_some();
    has_progress.then(|| {
        format!(
            "print-progress:{}:{}:{}:{}:{}:{}",
            job.job.id,
            input.observed_at,
            input.gcode_state.as_deref().unwrap_or(""),
            input
                .percent
                .map(|value| value.to_string())
                .unwrap_or_default(),
            input
                .current_layer
                .map(|value| value.to_string())
                .unwrap_or_default(),
            input
                .total_layers
                .map(|value| value.to_string())
                .unwrap_or_default()
        )
    })
}

fn terminal_event(
    input: &ApplyPrintReport,
    job: &JobWithArtifact,
    update: &PrintUpdate,
) -> Option<MachineEventInsert> {
    let status = update.print_status.parse::<PrintStatus>().ok()?;
    is_terminal_status(&status).then(|| MachineEventInsert {
        event_key: format!("print-terminal:{}:{}", job.job.id, status.as_str()),
        kind: "print_terminal".to_string(),
        severity: if status == PrintStatus::Completed {
            "info".to_string()
        } else {
            "error".to_string()
        },
        message: update
            .print_error
            .clone()
            .unwrap_or_else(|| format!("print {}", status.as_str())),
        code: None,
        payload_json: progress_payload(input),
    })
}

fn diagnostic_events(
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
    job: Option<&JobWithArtifact>,
) -> Vec<MachineEventInsert> {
    input
        .diagnostics
        .iter()
        .map(|diagnostic| {
            let code_or_hash = code_or_message_hash(diagnostic);
            let event_key = match job {
                Some(_) if diagnostic.kind == "hms" => {
                    format!("hms:{}:{}:{}", printer.id, code_or_hash, input.observed_at)
                }
                Some(job) if diagnostic.kind == "print_error" => {
                    format!(
                        "print-error:{}:{}:{}",
                        job.job.id, code_or_hash, input.observed_at
                    )
                }
                Some(job) => format!(
                    "machine:{}:{}:{}:{}",
                    job.job.id, diagnostic.kind, code_or_hash, input.observed_at
                ),
                None => format!(
                    "machine:{}:{}:{}:{}",
                    printer.id, diagnostic.kind, code_or_hash, input.observed_at
                ),
            };
            MachineEventInsert {
                event_key,
                kind: diagnostic.kind.clone(),
                severity: diagnostic.severity.clone(),
                message: diagnostic.message.clone(),
                code: diagnostic.code.clone(),
                payload_json: diagnostic.payload_json.clone(),
            }
        })
        .collect()
}

fn code_or_message_hash(diagnostic: &PrintReportDiagnostic) -> String {
    diagnostic
        .code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            let mut hash = Sha256::new();
            hash.update(diagnostic.message.trim().as_bytes());
            hash.update(b"\n");
            hash.update(diagnostic.payload_json.trim().as_bytes());
            format!("{:x}", hash.finalize())[..16].to_string()
        })
}

fn progress_message(input: &ApplyPrintReport) -> String {
    match (input.gcode_state.as_deref(), input.percent) {
        (Some(state), Some(percent)) => format!("print {state} {percent}%"),
        (Some(state), None) => format!("print {state}"),
        (None, Some(percent)) => format!("print progress {percent}%"),
        (None, None) => "print progress".to_string(),
    }
}

fn progress_payload(input: &ApplyPrintReport) -> String {
    serde_json::json!({
        "gcode_state": input.gcode_state,
        "percent": input.percent,
        "remaining_time_minutes": input.remaining_time_minutes,
        "current_layer": input.current_layer,
        "total_layers": input.total_layers,
        "gcode_file": input.gcode_file,
        "subtask_name": input.subtask_name,
    })
    .to_string()
}
