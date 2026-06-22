use anyhow::Context;
use pandar_core::{
    AgentId, CommandId, CommandRecord, CommandStatus, Job, JobArtifact, JobId, JobStatus, TenantId,
};

mod create;
pub mod rows;
mod transitions;

use crate::{
    db::Database,
    repositories::{RepositoryError, RepositoryResult},
};

use create::{create_print_job_postgres, create_print_job_sqlite};
use rows::{
    JOB_POSTGRES_BY_COMMAND, JOB_SQLITE_BY_COMMAND, JOB_WITH_ARTIFACT_POSTGRES_GET,
    JOB_WITH_ARTIFACT_POSTGRES_LIST, JOB_WITH_ARTIFACT_SQLITE_GET, JOB_WITH_ARTIFACT_SQLITE_LIST,
    job_from_postgres_row, job_from_sqlite_row, job_with_artifact_from_postgres_row,
    job_with_artifact_from_sqlite_row,
};

#[derive(Debug, Clone)]
pub struct JobRepository {
    database: Database,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatePrintJob {
    pub tenant_id: TenantId,
    pub printer_id: String,
    pub agent_id: AgentId,
    pub artifact_id: String,
    pub artifact_filename: String,
    pub artifact_content_type: String,
    pub artifact_size_bytes: u64,
    pub artifact_storage_path: String,
    pub plate_id: u32,
    pub use_ams: bool,
    pub flow_cali: bool,
    pub timelapse: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobWithArtifact {
    pub job: Job,
    pub artifact: JobArtifact,
}

impl JobRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn create_print_job(
        &self,
        input: CreatePrintJob,
    ) -> RepositoryResult<JobWithArtifact> {
        match &self.database {
            Database::Sqlite(pool) => {
                let mut transaction = pool
                    .begin()
                    .await
                    .context("failed to begin SQLite print job transaction")?;
                let created = create_print_job_sqlite(&mut transaction, input).await;
                match created {
                    Ok(created) => {
                        transaction
                            .commit()
                            .await
                            .context("failed to commit SQLite print job transaction")?;
                        Ok(created)
                    }
                    Err(err) => {
                        transaction
                            .rollback()
                            .await
                            .context("failed to roll back SQLite print job transaction")?;
                        Err(err)
                    }
                }
            }
            Database::Postgres(pool) => {
                let mut transaction = pool
                    .begin()
                    .await
                    .context("failed to begin PostgreSQL print job transaction")?;
                let created = create_print_job_postgres(&mut transaction, input).await;
                match created {
                    Ok(created) => {
                        transaction
                            .commit()
                            .await
                            .context("failed to commit PostgreSQL print job transaction")?;
                        Ok(created)
                    }
                    Err(err) => {
                        transaction
                            .rollback()
                            .await
                            .context("failed to roll back PostgreSQL print job transaction")?;
                        Err(err)
                    }
                }
            }
        }
    }

    pub async fn list_for_tenant(
        &self,
        tenant_id: TenantId,
    ) -> RepositoryResult<Vec<JobWithArtifact>> {
        if !tenant_exists(&self.database, tenant_id).await? {
            return Err(RepositoryError::MissingTenant);
        }

        match &self.database {
            Database::Sqlite(pool) => {
                let rows = sqlx::query(JOB_WITH_ARTIFACT_SQLITE_LIST)
                    .bind(tenant_id.to_string())
                    .fetch_all(pool)
                    .await
                    .context("failed to list SQLite print jobs")?;
                rows.into_iter()
                    .map(job_with_artifact_from_sqlite_row)
                    .collect()
            }
            Database::Postgres(pool) => {
                let rows = sqlx::query(JOB_WITH_ARTIFACT_POSTGRES_LIST)
                    .bind(tenant_id.to_string())
                    .fetch_all(pool)
                    .await
                    .context("failed to list PostgreSQL print jobs")?;
                rows.into_iter()
                    .map(job_with_artifact_from_postgres_row)
                    .collect()
            }
        }
    }

    pub async fn get_for_tenant(
        &self,
        tenant_id: TenantId,
        job_id: JobId,
    ) -> RepositoryResult<Option<JobWithArtifact>> {
        match &self.database {
            Database::Sqlite(pool) => {
                let row = sqlx::query(JOB_WITH_ARTIFACT_SQLITE_GET)
                    .bind(tenant_id.to_string())
                    .bind(job_id.to_string())
                    .fetch_optional(pool)
                    .await
                    .context("failed to get SQLite print job")?;
                row.map(job_with_artifact_from_sqlite_row).transpose()
            }
            Database::Postgres(pool) => {
                let row = sqlx::query(JOB_WITH_ARTIFACT_POSTGRES_GET)
                    .bind(tenant_id.to_string())
                    .bind(job_id.to_string())
                    .fetch_optional(pool)
                    .await
                    .context("failed to get PostgreSQL print job")?;
                row.map(job_with_artifact_from_postgres_row).transpose()
            }
        }
    }

    pub async fn mark_for_command(
        &self,
        command_id: CommandId,
        status: JobStatus,
        error: Option<String>,
    ) -> RepositoryResult<Option<Job>> {
        let updated = match &self.database {
            Database::Sqlite(pool) => sqlx::query(
                "UPDATE jobs SET status = ?2, error = ?3, updated_at = ?4
                     WHERE command_id = ?1 AND status NOT IN ('succeeded', 'failed')",
            )
            .bind(command_id.to_string())
            .bind(status.as_str())
            .bind(error.as_deref())
            .bind(pandar_core::created_at_now())
            .execute(pool)
            .await
            .context("failed to update SQLite job for command")?
            .rows_affected(),
            Database::Postgres(pool) => sqlx::query(
                "UPDATE jobs SET status = $2, error = $3, updated_at = $4
                     WHERE command_id = $1 AND status NOT IN ('succeeded', 'failed')",
            )
            .bind(command_id.to_string())
            .bind(status.as_str())
            .bind(error.as_deref())
            .bind(pandar_core::created_at_now())
            .execute(pool)
            .await
            .context("failed to update PostgreSQL job for command")?
            .rows_affected(),
        };

        if updated == 0 && self.get_by_command(command_id).await?.is_none() {
            return Ok(None);
        }

        self.get_by_command(command_id).await
    }

    pub async fn mark_print_sent(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<CommandRecord> {
        self.transition_print_command(transitions::PrintCommandTransition {
            command_id,
            tenant_id,
            agent_id,
            command_status: CommandStatus::Sent,
            job_status: JobStatus::Sent,
            error: None,
            allowed_statuses: &[CommandStatus::Queued],
            action: "send",
        })
        .await
    }

    pub async fn mark_print_acknowledged(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<CommandRecord> {
        self.transition_print_command(transitions::PrintCommandTransition {
            command_id,
            tenant_id,
            agent_id,
            command_status: CommandStatus::Acknowledged,
            job_status: JobStatus::Acknowledged,
            error: None,
            allowed_statuses: &[CommandStatus::Sent],
            action: "acknowledge",
        })
        .await
    }

    pub async fn mark_print_failed(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
        error: String,
    ) -> RepositoryResult<CommandRecord> {
        self.transition_print_command(transitions::PrintCommandTransition {
            command_id,
            tenant_id,
            agent_id,
            command_status: CommandStatus::Failed,
            job_status: JobStatus::Failed,
            error: Some(error),
            allowed_statuses: &[CommandStatus::Sent, CommandStatus::Acknowledged],
            action: "fail",
        })
        .await
    }

    pub async fn mark_print_succeeded(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<CommandRecord> {
        self.transition_print_command(transitions::PrintCommandTransition {
            command_id,
            tenant_id,
            agent_id,
            command_status: CommandStatus::Succeeded,
            job_status: JobStatus::Succeeded,
            error: None,
            allowed_statuses: &[CommandStatus::Sent, CommandStatus::Acknowledged],
            action: "succeed",
        })
        .await
    }

    async fn transition_print_command(
        &self,
        transition: transitions::PrintCommandTransition<'_>,
    ) -> RepositoryResult<CommandRecord> {
        match &self.database {
            Database::Sqlite(pool) => {
                let mut transaction = pool
                    .begin()
                    .await
                    .context("failed to begin SQLite print command transition transaction")?;
                let command =
                    transitions::transition_print_command_sqlite(&mut transaction, transition)
                        .await;
                match command {
                    Ok(command) => {
                        transaction
                            .commit()
                            .await
                            .context("failed to commit SQLite print command transition")?;
                        Ok(command)
                    }
                    Err(err) => {
                        transaction.rollback().await.context(
                            "failed to roll back SQLite print command transition transaction",
                        )?;
                        Err(err)
                    }
                }
            }
            Database::Postgres(pool) => {
                let mut transaction = pool
                    .begin()
                    .await
                    .context("failed to begin PostgreSQL print command transition transaction")?;
                let command =
                    transitions::transition_print_command_postgres(&mut transaction, transition)
                        .await;
                match command {
                    Ok(command) => {
                        transaction
                            .commit()
                            .await
                            .context("failed to commit PostgreSQL print command transition")?;
                        Ok(command)
                    }
                    Err(err) => {
                        transaction.rollback().await.context(
                            "failed to roll back PostgreSQL print command transition transaction",
                        )?;
                        Err(err)
                    }
                }
            }
        }
    }

    async fn get_by_command(&self, command_id: CommandId) -> RepositoryResult<Option<Job>> {
        match &self.database {
            Database::Sqlite(pool) => {
                let row = sqlx::query(JOB_SQLITE_BY_COMMAND)
                    .bind(command_id.to_string())
                    .fetch_optional(pool)
                    .await
                    .context("failed to get SQLite job by command")?;
                row.map(|row| job_from_sqlite_row(&row)).transpose()
            }
            Database::Postgres(pool) => {
                let row = sqlx::query(JOB_POSTGRES_BY_COMMAND)
                    .bind(command_id.to_string())
                    .fetch_optional(pool)
                    .await
                    .context("failed to get PostgreSQL job by command")?;
                row.map(|row| job_from_postgres_row(&row)).transpose()
            }
        }
    }
}

async fn tenant_exists(database: &Database, tenant_id: TenantId) -> RepositoryResult<bool> {
    let exists = match database {
        Database::Sqlite(pool) => {
            sqlx::query_scalar::<_, i64>("SELECT 1 FROM tenants WHERE id = ?1")
                .bind(tenant_id.to_string())
                .fetch_optional(pool)
                .await
        }
        Database::Postgres(pool) => {
            sqlx::query_scalar::<_, i64>("SELECT 1 FROM tenants WHERE id = $1")
                .bind(tenant_id.to_string())
                .fetch_optional(pool)
                .await
        }
    }
    .context("failed to check tenant existence for job repository")?;

    Ok(exists.is_some())
}
