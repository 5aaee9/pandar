use anyhow::Context;
use pandar_core::PrintStatus;
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, SqliteConnection};

use crate::repositories::{JobWithArtifact, RepositoryResult};

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

pub(super) async fn sqlite_insert_job_events(
    connection: &mut SqliteConnection,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
    job: &JobWithArtifact,
    update: &PrintUpdate,
) -> RepositoryResult<bool> {
    let mut inserted = false;
    if progress_event_key(job, input).is_some() {
        inserted |= sqlite_insert_event(
            connection,
            input,
            printer,
            Some(job),
            progress_event(input, job)?,
        )
        .await?;
    }
    if let Some(event) = terminal_event(input, job, update) {
        inserted |= sqlite_insert_event(connection, input, printer, Some(job), event).await?;
    }
    for event in diagnostic_events(input, printer, Some(job)) {
        inserted |= sqlite_insert_event(connection, input, printer, Some(job), event).await?;
    }
    Ok(inserted)
}

pub(super) async fn postgres_insert_job_events(
    connection: &mut PgConnection,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
    job: &JobWithArtifact,
    update: &PrintUpdate,
) -> RepositoryResult<bool> {
    let mut inserted = false;
    if progress_event_key(job, input).is_some() {
        inserted |= postgres_insert_event(
            connection,
            input,
            printer,
            Some(job),
            progress_event(input, job)?,
        )
        .await?;
    }
    if let Some(event) = terminal_event(input, job, update) {
        inserted |= postgres_insert_event(connection, input, printer, Some(job), event).await?;
    }
    for event in diagnostic_events(input, printer, Some(job)) {
        inserted |= postgres_insert_event(connection, input, printer, Some(job), event).await?;
    }
    Ok(inserted)
}

pub(super) async fn sqlite_insert_printer_events(
    connection: &mut SqliteConnection,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
) -> RepositoryResult<bool> {
    let mut inserted = false;
    for event in diagnostic_events(input, printer, None) {
        inserted |= sqlite_insert_event(connection, input, printer, None, event).await?;
    }
    Ok(inserted)
}

pub(super) async fn postgres_insert_printer_events(
    connection: &mut PgConnection,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
) -> RepositoryResult<bool> {
    let mut inserted = false;
    for event in diagnostic_events(input, printer, None) {
        inserted |= postgres_insert_event(connection, input, printer, None, event).await?;
    }
    Ok(inserted)
}

async fn sqlite_insert_event(
    connection: &mut SqliteConnection,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
    job: Option<&JobWithArtifact>,
    event: MachineEventInsert,
) -> RepositoryResult<bool> {
    let result = sqlx::query(
        "INSERT OR IGNORE INTO machine_events (id, tenant_id, agent_id, printer_id, job_id, event_key, kind, severity, message, code, payload_json, observed_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(input.tenant_id.to_string())
    .bind(input.agent_id.to_string())
    .bind(&printer.id)
    .bind(job.map(|job| job.job.id.to_string()))
    .bind(event.event_key)
    .bind(event.kind)
    .bind(event.severity)
    .bind(event.message)
    .bind(event.code)
    .bind(event.payload_json)
    .bind(&input.observed_at)
    .bind(pandar_core::created_at_now())
    .execute(&mut *connection)
    .await
    .context("failed to insert SQLite machine event")?;
    Ok(result.rows_affected() > 0)
}

async fn postgres_insert_event(
    connection: &mut PgConnection,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
    job: Option<&JobWithArtifact>,
    event: MachineEventInsert,
) -> RepositoryResult<bool> {
    let result = sqlx::query(
        "INSERT INTO machine_events (id, tenant_id, agent_id, printer_id, job_id, event_key, kind, severity, message, code, payload_json, observed_at, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
         ON CONFLICT (tenant_id, event_key) DO NOTHING",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(input.tenant_id.to_string())
    .bind(input.agent_id.to_string())
    .bind(&printer.id)
    .bind(job.map(|job| job.job.id.to_string()))
    .bind(event.event_key)
    .bind(event.kind)
    .bind(event.severity)
    .bind(event.message)
    .bind(event.code)
    .bind(event.payload_json)
    .bind(&input.observed_at)
    .bind(pandar_core::created_at_now())
    .execute(&mut *connection)
    .await
    .context("failed to insert PostgreSQL machine event")?;
    Ok(result.rows_affected() > 0)
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
