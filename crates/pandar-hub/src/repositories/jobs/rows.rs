use anyhow::Context;
use pandar_core::{
    AgentId, CommandId, Job, JobArtifact, JobArtifactParts, JobId, JobParts, TenantId,
};
use sqlx::{Row, postgres::PgRow, sqlite::SqliteRow};

use crate::repositories::{JobWithArtifact, RepositoryError, RepositoryResult};

pub const JOB_WITH_ARTIFACT_SQLITE_LIST: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.print_status, j.printer_state, j.progress_percent, j.remaining_time_minutes, j.current_layer, j.total_layers, j.active_file, j.last_progress_percent, j.last_layer, j.print_error, j.print_started_at, j.print_finished_at, j.print_updated_at, j.created_at, j.updated_at, a.id AS artifact_row_id, a.tenant_id AS artifact_tenant_id, a.filename, a.content_type, a.size_bytes, a.storage_path, a.created_at AS artifact_created_at FROM jobs j JOIN job_artifacts a ON a.id = j.artifact_id WHERE j.tenant_id = ?1 ORDER BY j.created_at DESC, j.id DESC";
pub const JOB_WITH_ARTIFACT_POSTGRES_LIST: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.print_status, j.printer_state, j.progress_percent, j.remaining_time_minutes, j.current_layer, j.total_layers, j.active_file, j.last_progress_percent, j.last_layer, j.print_error, j.print_started_at, j.print_finished_at, j.print_updated_at, j.created_at, j.updated_at, a.id AS artifact_row_id, a.tenant_id AS artifact_tenant_id, a.filename, a.content_type, a.size_bytes, a.storage_path, a.created_at AS artifact_created_at FROM jobs j JOIN job_artifacts a ON a.id = j.artifact_id WHERE j.tenant_id = $1 ORDER BY j.created_at DESC, j.id DESC";
pub const JOB_WITH_ARTIFACT_SQLITE_GET: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.print_status, j.printer_state, j.progress_percent, j.remaining_time_minutes, j.current_layer, j.total_layers, j.active_file, j.last_progress_percent, j.last_layer, j.print_error, j.print_started_at, j.print_finished_at, j.print_updated_at, j.created_at, j.updated_at, a.id AS artifact_row_id, a.tenant_id AS artifact_tenant_id, a.filename, a.content_type, a.size_bytes, a.storage_path, a.created_at AS artifact_created_at FROM jobs j JOIN job_artifacts a ON a.id = j.artifact_id WHERE j.tenant_id = ?1 AND j.id = ?2";
pub const JOB_WITH_ARTIFACT_POSTGRES_GET: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.print_status, j.printer_state, j.progress_percent, j.remaining_time_minutes, j.current_layer, j.total_layers, j.active_file, j.last_progress_percent, j.last_layer, j.print_error, j.print_started_at, j.print_finished_at, j.print_updated_at, j.created_at, j.updated_at, a.id AS artifact_row_id, a.tenant_id AS artifact_tenant_id, a.filename, a.content_type, a.size_bytes, a.storage_path, a.created_at AS artifact_created_at FROM jobs j JOIN job_artifacts a ON a.id = j.artifact_id WHERE j.tenant_id = $1 AND j.id = $2";
pub const JOB_SQLITE_BY_COMMAND: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.print_status, j.printer_state, j.progress_percent, j.remaining_time_minutes, j.current_layer, j.total_layers, j.active_file, j.last_progress_percent, j.last_layer, j.print_error, j.print_started_at, j.print_finished_at, j.print_updated_at, j.created_at, j.updated_at FROM jobs j WHERE j.command_id = ?1";
pub const JOB_POSTGRES_BY_COMMAND: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.print_status, j.printer_state, j.progress_percent, j.remaining_time_minutes, j.current_layer, j.total_layers, j.active_file, j.last_progress_percent, j.last_layer, j.print_error, j.print_started_at, j.print_finished_at, j.print_updated_at, j.created_at, j.updated_at FROM jobs j WHERE j.command_id = $1";

pub fn job_with_artifact_from_sqlite_row(row: SqliteRow) -> RepositoryResult<JobWithArtifact> {
    Ok(JobWithArtifact {
        job: job_from_sqlite_row(&row)?,
        artifact: artifact_from_sqlite_row(&row)?,
    })
}

pub fn job_with_artifact_from_postgres_row(row: PgRow) -> RepositoryResult<JobWithArtifact> {
    Ok(JobWithArtifact {
        job: job_from_postgres_row(&row)?,
        artifact: artifact_from_postgres_row(&row)?,
    })
}

pub fn job_from_sqlite_row(row: &SqliteRow) -> RepositoryResult<Job> {
    job_from_row_parts(JobRowParts {
        id: row.get("id"),
        tenant_id: row.get("tenant_id"),
        printer_id: row.get("printer_id"),
        agent_id: row.get("agent_id"),
        artifact_id: row.get("artifact_id"),
        command_id: row.get("command_id"),
        status: row.get("status"),
        error: row.get("error"),
        print_status: row.get("print_status"),
        printer_state: row.get("printer_state"),
        progress_percent: sqlite_u8(row, "progress_percent")?,
        remaining_time_minutes: sqlite_u32(row, "remaining_time_minutes")?,
        current_layer: sqlite_u32(row, "current_layer")?,
        total_layers: sqlite_u32(row, "total_layers")?,
        active_file: row.get("active_file"),
        last_progress_percent: sqlite_u8(row, "last_progress_percent")?,
        last_layer: sqlite_u32(row, "last_layer")?,
        print_error: row.get("print_error"),
        print_started_at: row.get("print_started_at"),
        print_finished_at: row.get("print_finished_at"),
        print_updated_at: row.get("print_updated_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

pub fn job_from_postgres_row(row: &PgRow) -> RepositoryResult<Job> {
    job_from_row_parts(JobRowParts {
        id: row.get("id"),
        tenant_id: row.get("tenant_id"),
        printer_id: row.get("printer_id"),
        agent_id: row.get("agent_id"),
        artifact_id: row.get("artifact_id"),
        command_id: row.get("command_id"),
        status: row.get("status"),
        error: row.get("error"),
        print_status: row.get("print_status"),
        printer_state: row.get("printer_state"),
        progress_percent: postgres_u8(row, "progress_percent")?,
        remaining_time_minutes: postgres_u32(row, "remaining_time_minutes")?,
        current_layer: postgres_u32(row, "current_layer")?,
        total_layers: postgres_u32(row, "total_layers")?,
        active_file: row.get("active_file"),
        last_progress_percent: postgres_u8(row, "last_progress_percent")?,
        last_layer: postgres_u32(row, "last_layer")?,
        print_error: row.get("print_error"),
        print_started_at: row.get("print_started_at"),
        print_finished_at: row.get("print_finished_at"),
        print_updated_at: row.get("print_updated_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn artifact_from_sqlite_row(row: &SqliteRow) -> RepositoryResult<JobArtifact> {
    artifact_from_row_parts(ArtifactRowParts {
        id: row.get("artifact_row_id"),
        tenant_id: row.get("artifact_tenant_id"),
        filename: row.get("filename"),
        content_type: row.get("content_type"),
        size_bytes: row.get::<i64, _>("size_bytes") as u64,
        storage_path: row.get("storage_path"),
        created_at: row.get("artifact_created_at"),
    })
}

fn artifact_from_postgres_row(row: &PgRow) -> RepositoryResult<JobArtifact> {
    artifact_from_row_parts(ArtifactRowParts {
        id: row.get("artifact_row_id"),
        tenant_id: row.get("artifact_tenant_id"),
        filename: row.get("filename"),
        content_type: row.get("content_type"),
        size_bytes: row.get::<i64, _>("size_bytes") as u64,
        storage_path: row.get("storage_path"),
        created_at: row.get("artifact_created_at"),
    })
}

struct JobRowParts {
    id: String,
    tenant_id: String,
    printer_id: String,
    agent_id: String,
    artifact_id: String,
    command_id: String,
    status: String,
    error: Option<String>,
    print_status: String,
    printer_state: Option<String>,
    progress_percent: Option<u8>,
    remaining_time_minutes: Option<u32>,
    current_layer: Option<u32>,
    total_layers: Option<u32>,
    active_file: Option<String>,
    last_progress_percent: Option<u8>,
    last_layer: Option<u32>,
    print_error: Option<String>,
    print_started_at: Option<String>,
    print_finished_at: Option<String>,
    print_updated_at: Option<String>,
    created_at: String,
    updated_at: String,
}

struct ArtifactRowParts {
    id: String,
    tenant_id: String,
    filename: String,
    content_type: String,
    size_bytes: u64,
    storage_path: String,
    created_at: String,
}

fn job_from_row_parts(parts: JobRowParts) -> RepositoryResult<Job> {
    let status_for_error = parts.status.clone();
    let print_status_for_error = parts.print_status.clone();
    Job::from_parts(JobParts {
        id: JobId::parse(&parts.id).map_err(anyhow::Error::from)?,
        tenant_id: TenantId::parse(&parts.tenant_id).map_err(anyhow::Error::from)?,
        printer_id: parts.printer_id,
        agent_id: AgentId::parse(&parts.agent_id).map_err(anyhow::Error::from)?,
        artifact_id: parts.artifact_id,
        command_id: CommandId::parse(&parts.command_id).map_err(anyhow::Error::from)?,
        status: parts.status,
        error: parts.error,
        print_status: parts.print_status,
        printer_state: parts.printer_state,
        progress_percent: parts.progress_percent,
        remaining_time_minutes: parts.remaining_time_minutes,
        current_layer: parts.current_layer,
        total_layers: parts.total_layers,
        active_file: parts.active_file,
        last_progress_percent: parts.last_progress_percent,
        last_layer: parts.last_layer,
        print_error: parts.print_error,
        print_started_at: parts.print_started_at,
        print_finished_at: parts.print_finished_at,
        print_updated_at: parts.print_updated_at,
        created_at: parts.created_at,
        updated_at: parts.updated_at,
    })
    .map_err(|err| match err {
        pandar_core::CoreError::InvalidJobStatus(_) => {
            RepositoryError::InvalidPersistedJobStatus(status_for_error)
        }
        pandar_core::CoreError::InvalidPrintStatus(_) => {
            RepositoryError::InvalidPersistedPrintStatus(print_status_for_error)
        }
        err => {
            RepositoryError::Database(anyhow::Error::from(err).context("failed to rehydrate job"))
        }
    })
}

fn sqlite_u8(row: &SqliteRow, name: &str) -> RepositoryResult<Option<u8>> {
    row.get::<Option<i64>, _>(name)
        .map(|value| {
            u8::try_from(value).map_err(|err| {
                RepositoryError::Database(
                    anyhow::Error::from(err)
                        .context(format!("invalid persisted SQLite u8 value for {name}")),
                )
            })
        })
        .transpose()
}

fn sqlite_u32(row: &SqliteRow, name: &str) -> RepositoryResult<Option<u32>> {
    row.get::<Option<i64>, _>(name)
        .map(|value| {
            u32::try_from(value).map_err(|err| {
                RepositoryError::Database(
                    anyhow::Error::from(err)
                        .context(format!("invalid persisted SQLite u32 value for {name}")),
                )
            })
        })
        .transpose()
}

fn postgres_u8(row: &PgRow, name: &str) -> RepositoryResult<Option<u8>> {
    row.get::<Option<i32>, _>(name)
        .map(|value| {
            u8::try_from(value).map_err(|err| {
                RepositoryError::Database(
                    anyhow::Error::from(err)
                        .context(format!("invalid persisted PostgreSQL u8 value for {name}")),
                )
            })
        })
        .transpose()
}

fn postgres_u32(row: &PgRow, name: &str) -> RepositoryResult<Option<u32>> {
    row.get::<Option<i32>, _>(name)
        .map(|value| {
            u32::try_from(value).map_err(|err| {
                RepositoryError::Database(
                    anyhow::Error::from(err)
                        .context(format!("invalid persisted PostgreSQL u32 value for {name}")),
                )
            })
        })
        .transpose()
}

fn artifact_from_row_parts(parts: ArtifactRowParts) -> RepositoryResult<JobArtifact> {
    JobArtifact::from_parts(JobArtifactParts {
        id: parts.id,
        tenant_id: TenantId::parse(&parts.tenant_id).map_err(anyhow::Error::from)?,
        filename: parts.filename,
        content_type: parts.content_type,
        size_bytes: parts.size_bytes,
        storage_path: parts.storage_path,
        created_at: parts.created_at,
    })
    .map_err(anyhow::Error::from)
    .context("failed to rehydrate job artifact")
    .map_err(RepositoryError::from)
}
