use anyhow::Context;
use pandar_core::{JobId, JobStatus, PrintStatus, StudioSubmissionId, TenantId};
use sea_orm::{
    ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
};

use crate::{
    entities::jobs,
    repositories::{JobRepository, JobWithArtifact, RepositoryResult},
};

#[cfg(test)]
pub(crate) mod test_pause;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudioTaskStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudioTaskQuery {
    pub printer_id: Option<String>,
    pub status: Option<StudioTaskStatus>,
    pub offset: u64,
    pub limit: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudioTaskPage {
    pub total: u64,
    pub jobs: Vec<JobWithArtifact>,
}

impl JobRepository {
    pub async fn list_studio_tasks(
        &self,
        tenant_id: TenantId,
        query: StudioTaskQuery,
    ) -> RepositoryResult<StudioTaskPage> {
        let tx = self
            .database
            .begin_snapshot_transaction()
            .await
            .context("failed to begin Studio task snapshot")?;
        let mut select =
            jobs::Entity::find().filter(jobs::Column::TenantId.eq(tenant_id.to_string()));
        if let Some(printer_id) = query.printer_id {
            select = select.filter(jobs::Column::PrinterId.eq(printer_id));
        }
        if let Some(status) = query.status {
            select = select.filter(status_condition(status));
        }
        let total = select
            .clone()
            .count(&tx)
            .await
            .context("failed to count Studio tasks")?;
        #[cfg(test)]
        test_pause::wait().await;
        let models = select
            .order_by_desc(jobs::Column::StudioSubmissionId)
            .offset(query.offset)
            .limit(query.limit)
            .all(&tx)
            .await
            .context("failed to list Studio tasks")?;
        let jobs = super::hydration::hydrate_jobs_with_artifacts(&tx, models).await?;
        tx.commit()
            .await
            .context("failed to commit Studio task snapshot")?;

        Ok(StudioTaskPage { total, jobs })
    }

    pub async fn get_by_studio_submission_id(
        &self,
        tenant_id: TenantId,
        studio_submission_id: StudioSubmissionId,
    ) -> RepositoryResult<Option<JobWithArtifact>> {
        let Some(job) = jobs::Entity::find()
            .filter(jobs::Column::TenantId.eq(tenant_id.to_string()))
            .filter(jobs::Column::StudioSubmissionId.eq(studio_submission_id.get()))
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to get Studio task")?
        else {
            return Ok(None);
        };
        let job_id = JobId::parse(&job.id)
            .map_err(anyhow::Error::from)
            .context("failed to parse Studio task job id")?;

        super::hydration::job_with_artifact_by_id(&self.database, tenant_id, job_id).await
    }
}

fn status_condition(status: StudioTaskStatus) -> Condition {
    match status {
        StudioTaskStatus::Completed => {
            Condition::all().add(jobs::Column::PrintStatus.eq(PrintStatus::Completed.as_str()))
        }
        StudioTaskStatus::Failed => Condition::all()
            .add(jobs::Column::PrintStatus.ne(PrintStatus::Completed.as_str()))
            .add(
                Condition::any()
                    .add(jobs::Column::Status.eq(JobStatus::Failed.as_str()))
                    .add(jobs::Column::PrintStatus.eq(PrintStatus::Failed.as_str()))
                    .add(jobs::Column::PrintStatus.eq(PrintStatus::Cancelled.as_str())),
            ),
        StudioTaskStatus::InProgress => Condition::all()
            .add(jobs::Column::PrintStatus.ne(PrintStatus::Completed.as_str()))
            .add(jobs::Column::Status.ne(JobStatus::Failed.as_str()))
            .add(jobs::Column::PrintStatus.ne(PrintStatus::Failed.as_str()))
            .add(jobs::Column::PrintStatus.ne(PrintStatus::Cancelled.as_str())),
    }
}
