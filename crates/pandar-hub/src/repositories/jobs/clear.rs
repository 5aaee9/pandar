use std::collections::{HashMap, HashSet};

use anyhow::Context;
use sea_orm::{ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter, QuerySelect};
use serde::Serialize;
use time::OffsetDateTime;

use crate::{
    artifacts::ArtifactStorage,
    db::Database,
    entities::{commands, job_artifacts, jobs},
    repositories::{AuditActor, JobRepository, RepositoryResult},
};

mod audit;

use crate::db::ConnectionDialectExt;
use audit::{DeleteJobAuditContext, insert_clear_audit};

enum ClearScope {
    Tenant,
    Job(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClearJobsOutcome {
    pub deleted_jobs: u64,
    pub retained_jobs: u64,
    pub deleted_commands: u64,
    pub deleted_artifacts: u64,
    pub deleted_artifact_bytes: u64,
}

impl JobRepository {
    pub async fn clear_for_tenant_with_audit(
        &self,
        artifact_storage: &dyn ArtifactStorage,
        tenant_id: pandar_core::TenantId,
        actor: AuditActor,
    ) -> RepositoryResult<ClearJobsOutcome> {
        self.clear_with_audit(artifact_storage, tenant_id, ClearScope::Tenant, actor)
            .await
    }

    pub async fn delete_clearable_for_tenant_with_audit(
        &self,
        artifact_storage: &dyn ArtifactStorage,
        tenant_id: pandar_core::TenantId,
        job_id: pandar_core::JobId,
        actor: AuditActor,
    ) -> RepositoryResult<ClearJobsOutcome> {
        self.clear_with_audit(
            artifact_storage,
            tenant_id,
            ClearScope::Job(job_id.to_string()),
            actor,
        )
        .await
    }

    async fn clear_with_audit(
        &self,
        artifact_storage: &dyn ArtifactStorage,
        tenant_id: pandar_core::TenantId,
        scope: ClearScope,
        actor: AuditActor,
    ) -> RepositoryResult<ClearJobsOutcome> {
        let tx = begin_clear_transaction(&self.database).await?;
        let target_id = match &scope {
            ClearScope::Tenant => None,
            ClearScope::Job(job_id) => Some(job_id),
        };
        let command_ids = jobs::Entity::find()
            .select_only()
            .column(jobs::Column::CommandId)
            .filter(jobs::Column::TenantId.eq(tenant_id.to_string()))
            .into_tuple::<String>()
            .all(&tx)
            .await
            .context("failed to select print commands for clearing")?
            .into_iter()
            .collect::<HashSet<_>>();
        let commands = locked_commands(&tx, command_ids).await?;
        let tenant_jobs = locked_tenant_jobs(&tx, tenant_id).await?;
        if target_id.is_some_and(|job_id| !tenant_jobs.iter().any(|job| &job.id == job_id)) {
            return Err(crate::repositories::RepositoryError::MissingJob);
        }
        let command_by_id = commands
            .iter()
            .map(|command| (command.id.as_str(), command))
            .collect::<HashMap<_, _>>();
        let now = OffsetDateTime::now_utc();
        let mut clearable_ids = tenant_jobs
            .iter()
            .map(|job| -> RepositoryResult<Option<String>> {
                let Some(command) = command_by_id.get(job.command_id.as_str()) else {
                    return Ok(None);
                };
                Ok(clearable_job(job, command, now)?.then(|| job.id.clone()))
            })
            .collect::<RepositoryResult<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<HashSet<_>>();
        if let Some(job_id) = target_id {
            if !clearable_ids.contains(job_id) {
                return Err(crate::repositories::RepositoryError::JobNotClearable);
            }
            clearable_ids.retain(|candidate| candidate == job_id);
        }
        let retained_jobs = tenant_jobs.len() as u64 - clearable_ids.len() as u64;

        let candidate_artifact_ids = tenant_jobs
            .iter()
            .filter(|job| clearable_ids.contains(&job.id))
            .map(|job| job.artifact_id.clone())
            .collect::<HashSet<_>>();
        let candidate_artifacts = locked_artifacts(&tx, tenant_id, &candidate_artifact_ids).await?;
        let delete_audit_context = target_id.map(|job_id| {
            let job = tenant_jobs
                .iter()
                .find(|job| &job.id == job_id)
                .expect("locked target job exists");
            let artifact = candidate_artifacts
                .iter()
                .find(|artifact| artifact.id == job.artifact_id)
                .expect("locked target artifact exists");
            DeleteJobAuditContext::from_models(job, artifact)
        });
        let artifact_references = locked_artifact_references(&tx, &candidate_artifact_ids).await?;
        let orphan_artifact_ids = candidate_artifact_ids
            .iter()
            .filter(|artifact_id| {
                artifact_references
                    .iter()
                    .filter(|job| &job.artifact_id == *artifact_id)
                    .all(|job| clearable_ids.contains(&job.id))
            })
            .cloned()
            .collect::<HashSet<_>>();
        let orphan_artifacts = candidate_artifacts
            .into_iter()
            .filter(|artifact| orphan_artifact_ids.contains(&artifact.id))
            .collect::<Vec<_>>();
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
        insert_clear_audit(
            &tx,
            tenant_id,
            actor,
            &scope,
            delete_audit_context.as_ref(),
            &outcome,
        )
        .await?;
        tx.commit()
            .await
            .context("failed to commit job clear transaction")?;
        Ok(outcome)
    }
}

async fn begin_clear_transaction(database: &Database) -> RepositoryResult<DatabaseTransaction> {
    database
        .begin_write_transaction()
        .await
        .context("failed to begin job clear transaction")
        .map_err(Into::into)
}

async fn locked_tenant_jobs(
    tx: &DatabaseTransaction,
    tenant_id: pandar_core::TenantId,
) -> RepositoryResult<Vec<jobs::Model>> {
    let query = jobs::Entity::find().filter(jobs::Column::TenantId.eq(tenant_id.to_string()));
    let rows = tx
        .lock_for_update(query)
        .all(tx)
        .await
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
    let rows = tx
        .lock_for_update(query)
        .all(tx)
        .await
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
    let rows = tx
        .lock_for_update(query)
        .all(tx)
        .await
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
    let rows = tx
        .lock_for_update(query)
        .all(tx)
        .await
        .context("failed to lock orphan job artifacts for clearing")?;
    Ok(rows)
}

fn clearable_job(
    job: &jobs::Model,
    command: &commands::Model,
    now: OffsetDateTime,
) -> RepositoryResult<bool> {
    if !matches!(job.status.as_str(), "succeeded" | "failed" | "cancelled")
        || !matches!(
            command.status.as_str(),
            "succeeded" | "failed" | "cancelled"
        )
        || command.kind != "print_project_file"
    {
        return Ok(false);
    }
    let clearable = match job.print_status.as_str() {
        "stalled" | "completed" | "failed" | "cancelled" => true,
        "pending"
            if job.print_started_at.is_none()
                && job.progress_percent.unwrap_or(0) == 0
                && job.current_layer.unwrap_or(0) == 0 =>
        {
            job.status == "failed"
                || (job.status == "succeeded"
                    && command.status == "succeeded"
                    && super::stalled::pending_job_is_stalled(command, now)?)
        }
        _ => false,
    };
    Ok(clearable)
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
        .context("failed to delete clearable jobs")
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
                && matches!(
                    command.status.as_str(),
                    "succeeded" | "failed" | "cancelled"
                )
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
