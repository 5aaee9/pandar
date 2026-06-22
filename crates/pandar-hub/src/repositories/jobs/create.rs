use anyhow::Context;
use pandar_core::{CommandId, Job, JobArtifact, JobArtifactParts, JobId, JobParts, JobStatus};
use sqlx::Row;

use crate::repositories::{
    CreatePrintJob, JobWithArtifact, PrintProjectFilePayload, RepositoryError, RepositoryResult,
    commands::inserts::{InsertCommand, insert_postgres, insert_sqlite},
};

pub async fn create_print_job_sqlite(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    input: CreatePrintJob,
) -> RepositoryResult<JobWithArtifact> {
    let printer = printer_for_agent_sqlite(&mut **transaction, &input).await?;
    let now = pandar_core::created_at_now();
    let job_id = JobId::new();
    let command_id = CommandId::new();
    insert_artifact_sqlite(&mut **transaction, &input, &now).await?;
    let payload = payload(&input, job_id, &printer.serial_number);
    let payload_json = serde_json::to_string(&payload)
        .context("failed to serialize print project file payload")?;
    insert_sqlite(
        &mut **transaction,
        InsertCommand {
            id: command_id,
            tenant_id: input.tenant_id,
            agent_id: input.agent_id,
            printer_id: Some(&input.printer_id),
            kind: "print_project_file",
            payload_json: &payload_json,
            created_at: &now,
        },
    )
    .await?;
    insert_job_sqlite(&mut **transaction, &input, job_id, command_id, &now).await?;
    build_created_job(input, job_id, command_id, now)
}

pub async fn create_print_job_postgres(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    input: CreatePrintJob,
) -> RepositoryResult<JobWithArtifact> {
    let printer = printer_for_agent_postgres(&mut **transaction, &input).await?;
    let now = pandar_core::created_at_now();
    let job_id = JobId::new();
    let command_id = CommandId::new();
    insert_artifact_postgres(&mut **transaction, &input, &now).await?;
    let payload = payload(&input, job_id, &printer.serial_number);
    let payload_json = serde_json::to_string(&payload)
        .context("failed to serialize print project file payload")?;
    insert_postgres(
        &mut **transaction,
        InsertCommand {
            id: command_id,
            tenant_id: input.tenant_id,
            agent_id: input.agent_id,
            printer_id: Some(&input.printer_id),
            kind: "print_project_file",
            payload_json: &payload_json,
            created_at: &now,
        },
    )
    .await?;
    insert_job_postgres(&mut **transaction, &input, job_id, command_id, &now).await?;
    build_created_job(input, job_id, command_id, now)
}

#[derive(Debug)]
struct PrintJobPrinter {
    serial_number: String,
}

async fn printer_for_agent_sqlite(
    executor: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    input: &CreatePrintJob,
) -> RepositoryResult<PrintJobPrinter> {
    let row = sqlx::query(
        "SELECT serial_number FROM printers WHERE id = ?1 AND tenant_id = ?2 AND agent_id = ?3",
    )
    .bind(&input.printer_id)
    .bind(input.tenant_id.to_string())
    .bind(input.agent_id.to_string())
    .fetch_optional(executor)
    .await
    .context("failed to verify SQLite print job printer ownership")?;

    row.map(|row| PrintJobPrinter {
        serial_number: row.get("serial_number"),
    })
    .ok_or(RepositoryError::MissingPrinter)
}

async fn printer_for_agent_postgres(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    input: &CreatePrintJob,
) -> RepositoryResult<PrintJobPrinter> {
    let row = sqlx::query(
        "SELECT serial_number FROM printers WHERE id = $1 AND tenant_id = $2 AND agent_id = $3",
    )
    .bind(&input.printer_id)
    .bind(input.tenant_id.to_string())
    .bind(input.agent_id.to_string())
    .fetch_optional(executor)
    .await
    .context("failed to verify PostgreSQL print job printer ownership")?;

    row.map(|row| PrintJobPrinter {
        serial_number: row.get("serial_number"),
    })
    .ok_or(RepositoryError::MissingPrinter)
}

async fn insert_artifact_sqlite(
    executor: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    input: &CreatePrintJob,
    now: &str,
) -> RepositoryResult<()> {
    sqlx::query(
        "INSERT INTO job_artifacts (id, tenant_id, filename, content_type, size_bytes, storage_path, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(&input.artifact_id)
    .bind(input.tenant_id.to_string())
    .bind(&input.artifact_filename)
    .bind(&input.artifact_content_type)
    .bind(input.artifact_size_bytes as i64)
    .bind(&input.artifact_storage_path)
    .bind(now)
    .execute(executor)
    .await
    .context("failed to insert SQLite job artifact")?;
    Ok(())
}

async fn insert_artifact_postgres(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    input: &CreatePrintJob,
    now: &str,
) -> RepositoryResult<()> {
    sqlx::query(
        "INSERT INTO job_artifacts (id, tenant_id, filename, content_type, size_bytes, storage_path, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&input.artifact_id)
    .bind(input.tenant_id.to_string())
    .bind(&input.artifact_filename)
    .bind(&input.artifact_content_type)
    .bind(input.artifact_size_bytes as i64)
    .bind(&input.artifact_storage_path)
    .bind(now)
    .execute(executor)
    .await
    .context("failed to insert PostgreSQL job artifact")?;
    Ok(())
}

async fn insert_job_sqlite(
    executor: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    input: &CreatePrintJob,
    job_id: JobId,
    command_id: CommandId,
    now: &str,
) -> RepositoryResult<()> {
    sqlx::query(
        "INSERT INTO jobs (id, tenant_id, printer_id, agent_id, artifact_id, command_id, status, error, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9)",
    )
    .bind(job_id.to_string())
    .bind(input.tenant_id.to_string())
    .bind(&input.printer_id)
    .bind(input.agent_id.to_string())
    .bind(&input.artifact_id)
    .bind(command_id.to_string())
    .bind(JobStatus::Queued.as_str())
    .bind(now)
    .bind(now)
    .execute(executor)
    .await
    .context("failed to insert SQLite print job")?;
    Ok(())
}

async fn insert_job_postgres(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    input: &CreatePrintJob,
    job_id: JobId,
    command_id: CommandId,
    now: &str,
) -> RepositoryResult<()> {
    sqlx::query(
        "INSERT INTO jobs (id, tenant_id, printer_id, agent_id, artifact_id, command_id, status, error, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, $8, $9)",
    )
    .bind(job_id.to_string())
    .bind(input.tenant_id.to_string())
    .bind(&input.printer_id)
    .bind(input.agent_id.to_string())
    .bind(&input.artifact_id)
    .bind(command_id.to_string())
    .bind(JobStatus::Queued.as_str())
    .bind(now)
    .bind(now)
    .execute(executor)
    .await
    .context("failed to insert PostgreSQL print job")?;
    Ok(())
}

fn payload(input: &CreatePrintJob, job_id: JobId, serial_number: &str) -> PrintProjectFilePayload {
    PrintProjectFilePayload {
        job_id: job_id.to_string(),
        artifact_id: input.artifact_id.clone(),
        printer_id: input.printer_id.clone(),
        serial_number: serial_number.to_string(),
        filename: input.artifact_filename.clone(),
        storage_path: input.artifact_storage_path.clone(),
        size_bytes: input.artifact_size_bytes,
        plate_id: input.plate_id,
        use_ams: input.use_ams,
        flow_cali: input.flow_cali,
        timelapse: input.timelapse,
    }
}

fn build_created_job(
    input: CreatePrintJob,
    job_id: JobId,
    command_id: CommandId,
    now: String,
) -> RepositoryResult<JobWithArtifact> {
    Ok(JobWithArtifact {
        artifact: JobArtifact::from_parts(JobArtifactParts {
            id: input.artifact_id.clone(),
            tenant_id: input.tenant_id,
            filename: input.artifact_filename,
            content_type: input.artifact_content_type,
            size_bytes: input.artifact_size_bytes,
            storage_path: input.artifact_storage_path,
            created_at: now.clone(),
        })
        .map_err(anyhow::Error::from)
        .context("failed to build print job artifact")?,
        job: Job::from_parts(JobParts {
            id: job_id,
            tenant_id: input.tenant_id,
            printer_id: input.printer_id,
            agent_id: input.agent_id,
            artifact_id: input.artifact_id,
            command_id,
            status: JobStatus::Queued.as_str().to_string(),
            error: None,
            created_at: now.clone(),
            updated_at: now,
        })
        .map_err(anyhow::Error::from)
        .context("failed to build print job")?,
    })
}
