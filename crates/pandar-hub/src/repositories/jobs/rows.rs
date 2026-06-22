use anyhow::Context;
use pandar_core::{
    AgentId, CommandId, Job, JobArtifact, JobArtifactParts, JobId, JobParts, TenantId,
};
use sqlx::{Row, postgres::PgRow, sqlite::SqliteRow};

use crate::repositories::{JobWithArtifact, RepositoryError, RepositoryResult};

pub const JOB_WITH_ARTIFACT_SQLITE_LIST: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.created_at, j.updated_at, a.id AS artifact_row_id, a.tenant_id AS artifact_tenant_id, a.filename, a.content_type, a.size_bytes, a.storage_path, a.created_at AS artifact_created_at FROM jobs j JOIN job_artifacts a ON a.id = j.artifact_id WHERE j.tenant_id = ?1 ORDER BY j.created_at DESC, j.id DESC";
pub const JOB_WITH_ARTIFACT_POSTGRES_LIST: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.created_at, j.updated_at, a.id AS artifact_row_id, a.tenant_id AS artifact_tenant_id, a.filename, a.content_type, a.size_bytes, a.storage_path, a.created_at AS artifact_created_at FROM jobs j JOIN job_artifacts a ON a.id = j.artifact_id WHERE j.tenant_id = $1 ORDER BY j.created_at DESC, j.id DESC";
pub const JOB_WITH_ARTIFACT_SQLITE_GET: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.created_at, j.updated_at, a.id AS artifact_row_id, a.tenant_id AS artifact_tenant_id, a.filename, a.content_type, a.size_bytes, a.storage_path, a.created_at AS artifact_created_at FROM jobs j JOIN job_artifacts a ON a.id = j.artifact_id WHERE j.tenant_id = ?1 AND j.id = ?2";
pub const JOB_WITH_ARTIFACT_POSTGRES_GET: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.created_at, j.updated_at, a.id AS artifact_row_id, a.tenant_id AS artifact_tenant_id, a.filename, a.content_type, a.size_bytes, a.storage_path, a.created_at AS artifact_created_at FROM jobs j JOIN job_artifacts a ON a.id = j.artifact_id WHERE j.tenant_id = $1 AND j.id = $2";
pub const JOB_SQLITE_BY_COMMAND: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.created_at, j.updated_at FROM jobs j WHERE j.command_id = ?1";
pub const JOB_POSTGRES_BY_COMMAND: &str = "SELECT j.id, j.tenant_id, j.printer_id, j.agent_id, j.artifact_id, j.command_id, j.status, j.error, j.created_at, j.updated_at FROM jobs j WHERE j.command_id = $1";

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
    Job::from_parts(JobParts {
        id: JobId::parse(&parts.id).map_err(anyhow::Error::from)?,
        tenant_id: TenantId::parse(&parts.tenant_id).map_err(anyhow::Error::from)?,
        printer_id: parts.printer_id,
        agent_id: AgentId::parse(&parts.agent_id).map_err(anyhow::Error::from)?,
        artifact_id: parts.artifact_id,
        command_id: CommandId::parse(&parts.command_id).map_err(anyhow::Error::from)?,
        status: parts.status,
        error: parts.error,
        created_at: parts.created_at,
        updated_at: parts.updated_at,
    })
    .map_err(|err| match err {
        pandar_core::CoreError::InvalidJobStatus(_) => {
            RepositoryError::InvalidPersistedJobStatus(status_for_error)
        }
        err => {
            RepositoryError::Database(anyhow::Error::from(err).context("failed to rehydrate job"))
        }
    })
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
