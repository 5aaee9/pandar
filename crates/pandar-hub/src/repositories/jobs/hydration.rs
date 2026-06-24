use std::collections::HashMap;

use anyhow::Context;
use pandar_core::{JobId, TenantId};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};

use crate::{
    db::Database,
    entities::{job_artifacts, jobs, tenants},
    repositories::{
        JobRepository, JobWithArtifact, RepositoryError, RepositoryResult,
        jobs::rows::{job_from_model_loading_usage, job_with_artifact_from_models},
    },
};

impl JobRepository {
    pub async fn list_for_tenant(
        &self,
        tenant_id: TenantId,
    ) -> RepositoryResult<Vec<JobWithArtifact>> {
        if !tenant_exists(&self.database, tenant_id).await? {
            return Err(RepositoryError::MissingTenant);
        }

        let connection = self.database.sea_orm_connection();
        let job_models = jobs::Entity::find()
            .filter(jobs::Column::TenantId.eq(tenant_id.to_string()))
            .order_by_desc(jobs::Column::CreatedAt)
            .order_by_desc(jobs::Column::Id)
            .all(&connection)
            .await
            .context("failed to list print jobs")?;

        hydrate_jobs_with_artifacts(&connection, job_models).await
    }

    pub async fn get_for_tenant(
        &self,
        tenant_id: TenantId,
        job_id: JobId,
    ) -> RepositoryResult<Option<JobWithArtifact>> {
        job_with_artifact_by_id(&self.database, tenant_id, job_id).await
    }
}

async fn tenant_exists(database: &Database, tenant_id: TenantId) -> RepositoryResult<bool> {
    let exists = tenants::Entity::find_by_id(tenant_id.to_string())
        .count(&database.sea_orm_connection())
        .await
        .context("failed to check tenant existence for job repository")?;

    Ok(exists > 0)
}

pub(crate) async fn job_with_artifact_by_id(
    database: &Database,
    tenant_id: TenantId,
    job_id: JobId,
) -> RepositoryResult<Option<JobWithArtifact>> {
    let Some(job) = jobs::Entity::find_by_id(job_id.to_string())
        .filter(jobs::Column::TenantId.eq(tenant_id.to_string()))
        .one(&database.sea_orm_connection())
        .await
        .context("failed to get print job")?
    else {
        return Ok(None);
    };

    let connection = database.sea_orm_connection();
    let artifact = artifact_for_job(&connection, &job).await?;
    let mut with_artifact = job_with_artifact_from_models(job.clone(), artifact)?;
    with_artifact.job = job_from_model_loading_usage(&connection, job).await?;
    Ok(Some(with_artifact))
}

async fn artifact_for_job<C>(
    connection: &C,
    job: &jobs::Model,
) -> RepositoryResult<job_artifacts::Model>
where
    C: sea_orm::ConnectionTrait,
{
    job_artifacts::Entity::find_by_id(&job.artifact_id)
        .one(connection)
        .await
        .context("failed to load job artifact")?
        .ok_or_else(|| {
            RepositoryError::Database(anyhow::anyhow!(
                "job {} references missing artifact {}",
                job.id,
                job.artifact_id
            ))
        })
}

pub(crate) async fn hydrate_jobs_with_artifacts<C>(
    connection: &C,
    job_models: Vec<jobs::Model>,
) -> RepositoryResult<Vec<JobWithArtifact>>
where
    C: sea_orm::ConnectionTrait,
{
    if job_models.is_empty() {
        return Ok(Vec::new());
    }

    let artifact_ids = job_models
        .iter()
        .map(|job| job.artifact_id.clone())
        .collect::<Vec<_>>();
    let artifacts = job_artifacts::Entity::find()
        .filter(job_artifacts::Column::Id.is_in(artifact_ids))
        .all(connection)
        .await
        .context("failed to bulk load job artifacts")?
        .into_iter()
        .map(|artifact| (artifact.id.clone(), artifact))
        .collect::<HashMap<_, _>>();

    let usage_models = crate::entities::job_filament_usages::Entity::find()
        .filter(
            crate::entities::job_filament_usages::Column::JobId.is_in(
                job_models
                    .iter()
                    .map(|job| job.id.clone())
                    .collect::<Vec<_>>(),
            ),
        )
        .all(connection)
        .await
        .context("failed to bulk load job filament usage")?;
    let mut usage_by_job = HashMap::new();
    for usage in usage_models {
        usage_by_job
            .entry(usage.job_id.clone())
            .or_insert_with(Vec::new)
            .push(usage);
    }

    job_models
        .into_iter()
        .map(|job| {
            let artifact = artifacts.get(&job.artifact_id).cloned().ok_or_else(|| {
                RepositoryError::Database(anyhow::anyhow!(
                    "job {} references missing artifact {}",
                    job.id,
                    job.artifact_id
                ))
            })?;
            let usage = usage_by_job
                .remove(&job.id)
                .unwrap_or_default()
                .into_iter()
                .map(super::rows::usage_from_model)
                .collect::<RepositoryResult<Vec<_>>>()?;
            job_with_artifact_from_models(job, artifact).map(|mut with_artifact| {
                with_artifact.job.filament_usage = usage;
                with_artifact
            })
        })
        .collect()
}
