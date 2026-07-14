use std::collections::{HashMap, HashSet};

use anyhow::Context;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseTransaction, EntityTrait, QueryFilter, QuerySelect,
    SqliteTransactionMode, TransactionOptions, TransactionTrait,
};
use serde::Serialize;

use crate::{
    artifacts::ArtifactStorage,
    db::Database,
    entities::{commands, job_artifacts, jobs},
    repositories::{
        AuditActor, JobRepository, RepositoryResult,
        audit::{audit_metadata, insert_audit_event_tx, record_audit_event},
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClearJobsOutcome {
    pub deleted_jobs: u64,
    pub retained_jobs: u64,
    pub deleted_commands: u64,
    pub deleted_artifacts: u64,
    pub deleted_artifact_bytes: u64,
}

#[derive(Serialize)]
struct ClearJobsAuditMetadata {
    deleted_jobs: u64,
    retained_jobs: u64,
    deleted_commands: u64,
    deleted_artifacts: u64,
    deleted_artifact_bytes: u64,
}

impl JobRepository {
    pub async fn clear_terminal_for_tenant_with_audit(
        &self,
        artifact_storage: &dyn ArtifactStorage,
        tenant_id: pandar_core::TenantId,
        actor: AuditActor,
    ) -> RepositoryResult<ClearJobsOutcome> {
        let tx = begin_clear_transaction(&self.database).await?;
        let tenant_jobs = locked_tenant_jobs(&tx, tenant_id).await?;
        let command_ids = tenant_jobs
            .iter()
            .map(|job| job.command_id.clone())
            .collect::<HashSet<_>>();
        let commands = locked_commands(&tx, command_ids).await?;
        let command_by_id = commands
            .iter()
            .map(|command| (command.id.as_str(), command))
            .collect::<HashMap<_, _>>();
        let clearable_ids = tenant_jobs
            .iter()
            .filter(|job| {
                command_by_id
                    .get(job.command_id.as_str())
                    .is_some_and(|command| clearable_job(job, command))
            })
            .map(|job| job.id.clone())
            .collect::<HashSet<_>>();
        let retained_jobs = tenant_jobs.len() as u64 - clearable_ids.len() as u64;

        let candidate_artifact_ids = tenant_jobs
            .iter()
            .filter(|job| clearable_ids.contains(&job.id))
            .map(|job| job.artifact_id.clone())
            .collect::<HashSet<_>>();
        let artifact_references = locked_artifact_references(&tx, &candidate_artifact_ids).await?;
        let orphan_artifact_ids = candidate_artifact_ids
            .into_iter()
            .filter(|artifact_id| {
                artifact_references
                    .iter()
                    .filter(|job| &job.artifact_id == artifact_id)
                    .all(|job| clearable_ids.contains(&job.id))
            })
            .collect::<HashSet<_>>();
        let orphan_artifacts = locked_artifacts(&tx, tenant_id, &orphan_artifact_ids).await?;
        let artifact_paths = orphan_artifacts
            .iter()
            .map(|artifact| artifact.storage_path.clone())
            .collect::<Vec<_>>();
        crate::cleanup::delete_artifacts(artifact_storage, &artifact_paths)
            .await
            .map_err(crate::repositories::RepositoryError::from)?;

        let deleted_jobs = delete_jobs(&tx, tenant_id, &clearable_ids).await?;
        let deleted_commands = delete_unreferenced_commands(&tx, commands, &clearable_ids).await?;
        let deleted_artifacts = delete_artifact_rows(&tx, tenant_id, &orphan_artifact_ids).await?;
        let deleted_artifact_bytes = orphan_artifacts
            .iter()
            .map(|artifact| artifact.size_bytes.max(0) as u64)
            .sum();
        let outcome = ClearJobsOutcome {
            deleted_jobs,
            retained_jobs,
            deleted_commands,
            deleted_artifacts,
            deleted_artifact_bytes,
        };
        insert_clear_audit(&tx, tenant_id, actor, &outcome).await?;
        tx.commit()
            .await
            .context("failed to commit terminal job clear transaction")?;
        Ok(outcome)
    }
}

async fn begin_clear_transaction(database: &Database) -> RepositoryResult<DatabaseTransaction> {
    database
        .sea_orm_connection()
        .begin_with_options(TransactionOptions {
            sqlite_transaction_mode: matches!(database, Database::Sqlite(_))
                .then_some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await
        .context("failed to begin terminal job clear transaction")
        .map_err(Into::into)
}

async fn locked_tenant_jobs(
    tx: &DatabaseTransaction,
    tenant_id: pandar_core::TenantId,
) -> RepositoryResult<Vec<jobs::Model>> {
    let query = jobs::Entity::find().filter(jobs::Column::TenantId.eq(tenant_id.to_string()));
    let rows = match tx.get_database_backend() {
        sea_orm::DatabaseBackend::Postgres => query.lock_exclusive().all(tx).await,
        _ => query.all(tx).await,
    }
    .context("failed to lock tenant jobs for clearing")?;
    Ok(rows)
}

async fn locked_commands(
    tx: &DatabaseTransaction,
    ids: HashSet<String>,
) -> RepositoryResult<Vec<commands::Model>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let query = commands::Entity::find().filter(commands::Column::Id.is_in(ids));
    let rows = match tx.get_database_backend() {
        sea_orm::DatabaseBackend::Postgres => query.lock_exclusive().all(tx).await,
        _ => query.all(tx).await,
    }
    .context("failed to lock print commands for clearing")?;
    Ok(rows)
}

async fn locked_artifact_references(
    tx: &DatabaseTransaction,
    artifact_ids: &HashSet<String>,
) -> RepositoryResult<Vec<jobs::Model>> {
    if artifact_ids.is_empty() {
        return Ok(Vec::new());
    }
    let query = jobs::Entity::find().filter(jobs::Column::ArtifactId.is_in(artifact_ids.clone()));
    let rows = match tx.get_database_backend() {
        sea_orm::DatabaseBackend::Postgres => query.lock_exclusive().all(tx).await,
        _ => query.all(tx).await,
    }
    .context("failed to lock artifact job references for clearing")?;
    Ok(rows)
}

async fn locked_artifacts(
    tx: &DatabaseTransaction,
    tenant_id: pandar_core::TenantId,
    ids: &HashSet<String>,
) -> RepositoryResult<Vec<job_artifacts::Model>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let query = job_artifacts::Entity::find()
        .filter(job_artifacts::Column::TenantId.eq(tenant_id.to_string()))
        .filter(job_artifacts::Column::Id.is_in(ids.clone()));
    let rows = match tx.get_database_backend() {
        sea_orm::DatabaseBackend::Postgres => query.lock_exclusive().all(tx).await,
        _ => query.all(tx).await,
    }
    .context("failed to lock orphan job artifacts for clearing")?;
    Ok(rows)
}

fn clearable_job(job: &jobs::Model, command: &commands::Model) -> bool {
    if !matches!(job.status.as_str(), "succeeded" | "failed")
        || !matches!(command.status.as_str(), "succeeded" | "failed")
        || command.kind != "print_project_file"
    {
        return false;
    }
    match job.print_status.as_str() {
        "completed" | "failed" | "cancelled" => true,
        "pending" if job.status == "failed" => {
            job.print_started_at.is_none()
                && job.progress_percent.unwrap_or(0) == 0
                && job.current_layer.unwrap_or(0) == 0
        }
        _ => false,
    }
}

async fn delete_jobs(
    tx: &DatabaseTransaction,
    tenant_id: pandar_core::TenantId,
    ids: &HashSet<String>,
) -> RepositoryResult<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    jobs::Entity::delete_many()
        .filter(jobs::Column::TenantId.eq(tenant_id.to_string()))
        .filter(jobs::Column::Id.is_in(ids.clone()))
        .exec(tx)
        .await
        .context("failed to delete terminal jobs")
        .map(|result| result.rows_affected)
        .map_err(Into::into)
}

async fn delete_unreferenced_commands(
    tx: &DatabaseTransaction,
    commands: Vec<commands::Model>,
    clearable_job_ids: &HashSet<String>,
) -> RepositoryResult<u64> {
    let candidate_ids = commands
        .into_iter()
        .filter(|command| {
            command.kind == "print_project_file"
                && matches!(command.status.as_str(), "succeeded" | "failed")
        })
        .map(|command| command.id)
        .collect::<HashSet<_>>();
    if candidate_ids.is_empty() || clearable_job_ids.is_empty() {
        return Ok(0);
    }
    let referenced = jobs::Entity::find()
        .filter(jobs::Column::CommandId.is_in(candidate_ids.clone()))
        .all(tx)
        .await
        .context("failed to check retained print command references")?
        .into_iter()
        .map(|job| job.command_id)
        .collect::<HashSet<_>>();
    let delete_ids = candidate_ids
        .difference(&referenced)
        .cloned()
        .collect::<HashSet<_>>();
    if delete_ids.is_empty() {
        return Ok(0);
    }
    commands::Entity::delete_many()
        .filter(commands::Column::Id.is_in(delete_ids))
        .exec(tx)
        .await
        .context("failed to delete terminal print commands")
        .map(|result| result.rows_affected)
        .map_err(Into::into)
}

async fn delete_artifact_rows(
    tx: &DatabaseTransaction,
    tenant_id: pandar_core::TenantId,
    ids: &HashSet<String>,
) -> RepositoryResult<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    job_artifacts::Entity::delete_many()
        .filter(job_artifacts::Column::TenantId.eq(tenant_id.to_string()))
        .filter(job_artifacts::Column::Id.is_in(ids.clone()))
        .exec(tx)
        .await
        .context("failed to delete orphan job artifact rows")
        .map(|result| result.rows_affected)
        .map_err(Into::into)
}

async fn insert_clear_audit(
    tx: &DatabaseTransaction,
    tenant_id: pandar_core::TenantId,
    actor: AuditActor,
    outcome: &ClearJobsOutcome,
) -> RepositoryResult<()> {
    let event = record_audit_event(
        tenant_id,
        actor,
        "job.clear",
        "job_collection",
        None,
        audit_metadata(ClearJobsAuditMetadata {
            deleted_jobs: outcome.deleted_jobs,
            retained_jobs: outcome.retained_jobs,
            deleted_commands: outcome.deleted_commands,
            deleted_artifacts: outcome.deleted_artifacts,
            deleted_artifact_bytes: outcome.deleted_artifact_bytes,
        }),
    );
    insert_audit_event_tx(tx, &event).await
}
