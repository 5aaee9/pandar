use anyhow::Context;
use pandar_core::{JobId, TenantId};
use sqlx::{PgConnection, Row, SqliteConnection};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::repositories::{
    JobWithArtifact, RepositoryResult,
    jobs::rows::{job_with_artifact_from_postgres_row, job_with_artifact_from_sqlite_row},
};

use super::ApplyPrintReport;

#[derive(Debug, Clone)]
pub(super) struct PrinterMatch {
    pub(super) id: String,
}

const SQLITE_JOB_BY_ID_FOR_PRINTER: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.print_status, j.printer_state, j.progress_percent, j.remaining_time_minutes, j.current_layer, j.total_layers, j.active_file, j.last_progress_percent, j.last_layer, j.print_error, j.print_started_at, j.print_finished_at, j.print_updated_at, j.created_at, j.updated_at, a.id AS artifact_row_id, a.tenant_id AS artifact_tenant_id, a.filename, a.content_type, a.size_bytes, a.storage_path, a.created_at AS artifact_created_at FROM jobs j JOIN job_artifacts a ON a.id = j.artifact_id WHERE j.tenant_id = ?1 AND j.agent_id = ?2 AND j.printer_id = ?3 AND j.id = ?4";
const POSTGRES_JOB_BY_ID_FOR_PRINTER: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.print_status, j.printer_state, j.progress_percent, j.remaining_time_minutes, j.current_layer, j.total_layers, j.active_file, j.last_progress_percent, j.last_layer, j.print_error, j.print_started_at, j.print_finished_at, j.print_updated_at, j.created_at, j.updated_at, a.id AS artifact_row_id, a.tenant_id AS artifact_tenant_id, a.filename, a.content_type, a.size_bytes, a.storage_path, a.created_at AS artifact_created_at FROM jobs j JOIN job_artifacts a ON a.id = j.artifact_id WHERE j.tenant_id = $1 AND j.agent_id = $2 AND j.printer_id = $3 AND j.id = $4";
const SQLITE_JOB_BY_ARTIFACT: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.print_status, j.printer_state, j.progress_percent, j.remaining_time_minutes, j.current_layer, j.total_layers, j.active_file, j.last_progress_percent, j.last_layer, j.print_error, j.print_started_at, j.print_finished_at, j.print_updated_at, j.created_at, j.updated_at, a.id AS artifact_row_id, a.tenant_id AS artifact_tenant_id, a.filename, a.content_type, a.size_bytes, a.storage_path, a.created_at AS artifact_created_at FROM jobs j JOIN job_artifacts a ON a.id = j.artifact_id WHERE j.tenant_id = ?1 AND j.agent_id = ?2 AND j.printer_id = ?3 AND j.artifact_id = ?4";
const POSTGRES_JOB_BY_ARTIFACT: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.print_status, j.printer_state, j.progress_percent, j.remaining_time_minutes, j.current_layer, j.total_layers, j.active_file, j.last_progress_percent, j.last_layer, j.print_error, j.print_started_at, j.print_finished_at, j.print_updated_at, j.created_at, j.updated_at, a.id AS artifact_row_id, a.tenant_id AS artifact_tenant_id, a.filename, a.content_type, a.size_bytes, a.storage_path, a.created_at AS artifact_created_at FROM jobs j JOIN job_artifacts a ON a.id = j.artifact_id WHERE j.tenant_id = $1 AND j.agent_id = $2 AND j.printer_id = $3 AND j.artifact_id = $4";
const SQLITE_ACTIVE_FILE_CANDIDATES: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.print_status, j.printer_state, j.progress_percent, j.remaining_time_minutes, j.current_layer, j.total_layers, j.active_file, j.last_progress_percent, j.last_layer, j.print_error, j.print_started_at, j.print_finished_at, j.print_updated_at, j.created_at, j.updated_at, a.id AS artifact_row_id, a.tenant_id AS artifact_tenant_id, a.filename, a.content_type, a.size_bytes, a.storage_path, a.created_at AS artifact_created_at FROM jobs j JOIN job_artifacts a ON a.id = j.artifact_id WHERE j.tenant_id = ?1 AND j.agent_id = ?2 AND j.printer_id = ?3 AND j.print_status IN ('pending', 'running') AND j.created_at >= ?4";
const POSTGRES_ACTIVE_FILE_CANDIDATES: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.print_status, j.printer_state, j.progress_percent, j.remaining_time_minutes, j.current_layer, j.total_layers, j.active_file, j.last_progress_percent, j.last_layer, j.print_error, j.print_started_at, j.print_finished_at, j.print_updated_at, j.created_at, j.updated_at, a.id AS artifact_row_id, a.tenant_id AS artifact_tenant_id, a.filename, a.content_type, a.size_bytes, a.storage_path, a.created_at AS artifact_created_at FROM jobs j JOIN job_artifacts a ON a.id = j.artifact_id WHERE j.tenant_id = $1 AND j.agent_id = $2 AND j.printer_id = $3 AND j.print_status IN ('pending', 'running') AND j.created_at >= $4";
const SQLITE_JOB_BY_ID: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.print_status, j.printer_state, j.progress_percent, j.remaining_time_minutes, j.current_layer, j.total_layers, j.active_file, j.last_progress_percent, j.last_layer, j.print_error, j.print_started_at, j.print_finished_at, j.print_updated_at, j.created_at, j.updated_at, a.id AS artifact_row_id, a.tenant_id AS artifact_tenant_id, a.filename, a.content_type, a.size_bytes, a.storage_path, a.created_at AS artifact_created_at FROM jobs j JOIN job_artifacts a ON a.id = j.artifact_id WHERE j.tenant_id = ?1 AND j.id = ?2";
const POSTGRES_JOB_BY_ID: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.print_status, j.printer_state, j.progress_percent, j.remaining_time_minutes, j.current_layer, j.total_layers, j.active_file, j.last_progress_percent, j.last_layer, j.print_error, j.print_started_at, j.print_finished_at, j.print_updated_at, j.created_at, j.updated_at, a.id AS artifact_row_id, a.tenant_id AS artifact_tenant_id, a.filename, a.content_type, a.size_bytes, a.storage_path, a.created_at AS artifact_created_at FROM jobs j JOIN job_artifacts a ON a.id = j.artifact_id WHERE j.tenant_id = $1 AND j.id = $2";

pub(super) async fn sqlite_printer_for_serial(
    connection: &mut SqliteConnection,
    input: &ApplyPrintReport,
) -> RepositoryResult<Option<PrinterMatch>> {
    sqlx::query(
        "SELECT id FROM printers WHERE tenant_id = ?1 AND agent_id = ?2 AND serial_number = ?3",
    )
    .bind(input.tenant_id.to_string())
    .bind(input.agent_id.to_string())
    .bind(&input.serial)
    .fetch_optional(&mut *connection)
    .await
    .context("failed to resolve SQLite print report printer")
    .map(|row| row.map(|row| PrinterMatch { id: row.get("id") }))
    .map_err(Into::into)
}

pub(super) async fn postgres_printer_for_serial(
    connection: &mut PgConnection,
    input: &ApplyPrintReport,
) -> RepositoryResult<Option<PrinterMatch>> {
    sqlx::query(
        "SELECT id FROM printers WHERE tenant_id = $1 AND agent_id = $2 AND serial_number = $3",
    )
    .bind(input.tenant_id.to_string())
    .bind(input.agent_id.to_string())
    .bind(&input.serial)
    .fetch_optional(&mut *connection)
    .await
    .context("failed to resolve PostgreSQL print report printer")
    .map(|row| row.map(|row| PrinterMatch { id: row.get("id") }))
    .map_err(Into::into)
}

pub(super) async fn sqlite_correlate_job(
    connection: &mut SqliteConnection,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
) -> RepositoryResult<Option<JobWithArtifact>> {
    if let Some(job_id) = input.job_id
        && let Some(job) = sqlite_job_by_id_for_printer(connection, input, printer, job_id).await?
    {
        return Ok(Some(job));
    }
    if let Some(job) =
        sqlite_job_by_artifact(connection, input, printer, input.artifact_id.as_deref()).await?
    {
        return Ok(Some(job));
    }
    if let Some(job) =
        sqlite_job_by_artifact(connection, input, printer, input.subtask_id.as_deref()).await?
    {
        return Ok(Some(job));
    }
    sqlite_job_by_active_file(connection, input, printer).await
}

pub(super) async fn postgres_correlate_job(
    connection: &mut PgConnection,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
) -> RepositoryResult<Option<JobWithArtifact>> {
    if let Some(job_id) = input.job_id
        && let Some(job) =
            postgres_job_by_id_for_printer(connection, input, printer, job_id).await?
    {
        return Ok(Some(job));
    }
    if let Some(job) =
        postgres_job_by_artifact(connection, input, printer, input.artifact_id.as_deref()).await?
    {
        return Ok(Some(job));
    }
    if let Some(job) =
        postgres_job_by_artifact(connection, input, printer, input.subtask_id.as_deref()).await?
    {
        return Ok(Some(job));
    }
    postgres_job_by_active_file(connection, input, printer).await
}

pub(super) async fn sqlite_job_by_id(
    connection: &mut SqliteConnection,
    tenant_id: TenantId,
    job_id: JobId,
) -> RepositoryResult<Option<JobWithArtifact>> {
    let row = sqlx::query(SQLITE_JOB_BY_ID)
        .bind(tenant_id.to_string())
        .bind(job_id.to_string())
        .fetch_optional(&mut *connection)
        .await
        .context("failed to get SQLite print report job")?;
    row.map(job_with_artifact_from_sqlite_row).transpose()
}

pub(super) async fn postgres_job_by_id(
    connection: &mut PgConnection,
    tenant_id: TenantId,
    job_id: JobId,
) -> RepositoryResult<Option<JobWithArtifact>> {
    let row = sqlx::query(POSTGRES_JOB_BY_ID)
        .bind(tenant_id.to_string())
        .bind(job_id.to_string())
        .fetch_optional(&mut *connection)
        .await
        .context("failed to get PostgreSQL print report job")?;
    row.map(job_with_artifact_from_postgres_row).transpose()
}

async fn sqlite_job_by_id_for_printer(
    connection: &mut SqliteConnection,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
    job_id: JobId,
) -> RepositoryResult<Option<JobWithArtifact>> {
    let row = sqlx::query(SQLITE_JOB_BY_ID_FOR_PRINTER)
        .bind(input.tenant_id.to_string())
        .bind(input.agent_id.to_string())
        .bind(&printer.id)
        .bind(job_id.to_string())
        .fetch_optional(&mut *connection)
        .await
        .context("failed to correlate SQLite print report by job id")?;
    row.map(job_with_artifact_from_sqlite_row).transpose()
}

async fn postgres_job_by_id_for_printer(
    connection: &mut PgConnection,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
    job_id: JobId,
) -> RepositoryResult<Option<JobWithArtifact>> {
    let row = sqlx::query(POSTGRES_JOB_BY_ID_FOR_PRINTER)
        .bind(input.tenant_id.to_string())
        .bind(input.agent_id.to_string())
        .bind(&printer.id)
        .bind(job_id.to_string())
        .fetch_optional(&mut *connection)
        .await
        .context("failed to correlate PostgreSQL print report by job id")?;
    row.map(job_with_artifact_from_postgres_row).transpose()
}

async fn sqlite_job_by_artifact(
    connection: &mut SqliteConnection,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
    artifact_id: Option<&str>,
) -> RepositoryResult<Option<JobWithArtifact>> {
    let Some(artifact_id) = artifact_id else {
        return Ok(None);
    };
    let row = sqlx::query(SQLITE_JOB_BY_ARTIFACT)
        .bind(input.tenant_id.to_string())
        .bind(input.agent_id.to_string())
        .bind(&printer.id)
        .bind(artifact_id)
        .fetch_optional(&mut *connection)
        .await
        .context("failed to correlate SQLite print report by artifact id")?;
    row.map(job_with_artifact_from_sqlite_row).transpose()
}

async fn postgres_job_by_artifact(
    connection: &mut PgConnection,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
    artifact_id: Option<&str>,
) -> RepositoryResult<Option<JobWithArtifact>> {
    let Some(artifact_id) = artifact_id else {
        return Ok(None);
    };
    let row = sqlx::query(POSTGRES_JOB_BY_ARTIFACT)
        .bind(input.tenant_id.to_string())
        .bind(input.agent_id.to_string())
        .bind(&printer.id)
        .bind(artifact_id)
        .fetch_optional(&mut *connection)
        .await
        .context("failed to correlate PostgreSQL print report by artifact id")?;
    row.map(job_with_artifact_from_postgres_row).transpose()
}

async fn sqlite_job_by_active_file(
    connection: &mut SqliteConnection,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
) -> RepositoryResult<Option<JobWithArtifact>> {
    let candidates = sqlite_active_file_candidates(connection, input, printer).await?;
    Ok(single_file_match(candidates, input))
}

async fn postgres_job_by_active_file(
    connection: &mut PgConnection,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
) -> RepositoryResult<Option<JobWithArtifact>> {
    let candidates = postgres_active_file_candidates(connection, input, printer).await?;
    Ok(single_file_match(candidates, input))
}

async fn sqlite_active_file_candidates(
    connection: &mut SqliteConnection,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
) -> RepositoryResult<Vec<JobWithArtifact>> {
    let cutoff = cutoff_observed_at(&input.observed_at)?;
    let rows = sqlx::query(SQLITE_ACTIVE_FILE_CANDIDATES)
        .bind(input.tenant_id.to_string())
        .bind(input.agent_id.to_string())
        .bind(&printer.id)
        .bind(cutoff)
        .fetch_all(&mut *connection)
        .await
        .context("failed to list SQLite active-file print report candidates")?;
    rows.into_iter()
        .map(job_with_artifact_from_sqlite_row)
        .collect()
}

async fn postgres_active_file_candidates(
    connection: &mut PgConnection,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
) -> RepositoryResult<Vec<JobWithArtifact>> {
    let cutoff = cutoff_observed_at(&input.observed_at)?;
    let rows = sqlx::query(POSTGRES_ACTIVE_FILE_CANDIDATES)
        .bind(input.tenant_id.to_string())
        .bind(input.agent_id.to_string())
        .bind(&printer.id)
        .bind(cutoff)
        .fetch_all(&mut *connection)
        .await
        .context("failed to list PostgreSQL active-file print report candidates")?;
    rows.into_iter()
        .map(job_with_artifact_from_postgres_row)
        .collect()
}

fn single_file_match(
    candidates: Vec<JobWithArtifact>,
    input: &ApplyPrintReport,
) -> Option<JobWithArtifact> {
    let report_basename = input.gcode_file.as_deref().and_then(basename);
    let subtask_name = input
        .subtask_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut matches = candidates.into_iter().filter(|candidate| {
        let filename = candidate.artifact.filename.trim();
        report_basename.is_some_and(|name| name == filename)
            || subtask_name.is_some_and(|name| name == filename_stem(filename))
    });
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn cutoff_observed_at(observed_at: &str) -> RepositoryResult<String> {
    let observed = OffsetDateTime::parse(observed_at, &Rfc3339)
        .context("failed to parse print report observed_at")?;
    (observed - Duration::hours(24))
        .format(&Rfc3339)
        .context("failed to format print report fallback cutoff")
        .map_err(Into::into)
}

fn basename(value: &str) -> Option<&str> {
    value
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .filter(|value| !value.is_empty())
}

fn filename_stem(filename: &str) -> &str {
    filename
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(filename)
}
