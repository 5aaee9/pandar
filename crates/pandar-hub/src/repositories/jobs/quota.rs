use anyhow::Context;
use pandar_core::{StudioPrintMetadata, TenantId};

use crate::repositories::{AuditActor, RepositoryResult};

use super::{
    ArtifactQuotaReservation, CreatePrintJob, JobRepository, JobWithArtifact, audit,
    write_transaction,
};

#[derive(Debug, Clone, Copy)]
pub struct ArtifactQuotaLimits {
    pub tenant_bytes: u64,
    pub tenant_count: u64,
    pub global_bytes: u64,
    pub global_count: u64,
}

impl JobRepository {
    pub(crate) async fn reserve_artifact_quota(
        &self,
        tenant_id: TenantId,
        upload_bytes: u64,
        quota: ArtifactQuotaLimits,
    ) -> RepositoryResult<ArtifactQuotaReservation> {
        let tx = write_transaction::begin(&self.database)
            .await
            .context("failed to begin artifact quota reservation transaction")?;
        audit::enforce_artifact_quota(&tx, tenant_id, upload_bytes, quota).await?;
        Ok(ArtifactQuotaReservation { tx })
    }

    pub async fn create_print_job_with_quota_and_audit(
        &self,
        input: CreatePrintJob,
        quota: ArtifactQuotaLimits,
        actor: AuditActor,
    ) -> RepositoryResult<JobWithArtifact> {
        audit::create_print_job_with_quota_and_audit(&self.database, input, quota, actor).await
    }

    pub async fn create_studio_print_job_with_quota_and_audit(
        &self,
        input: CreatePrintJob,
        metadata: StudioPrintMetadata,
        quota: ArtifactQuotaLimits,
        actor: AuditActor,
    ) -> RepositoryResult<JobWithArtifact> {
        audit::create_studio_print_job_with_quota_and_audit(
            &self.database,
            input,
            metadata,
            quota,
            actor,
        )
        .await
    }
}

impl ArtifactQuotaReservation {
    pub(crate) async fn create_print_job_with_audit(
        self,
        input: CreatePrintJob,
        actor: AuditActor,
    ) -> RepositoryResult<JobWithArtifact> {
        self.complete(input, None, actor).await
    }

    pub(crate) async fn create_studio_print_job_with_audit(
        self,
        input: CreatePrintJob,
        metadata: StudioPrintMetadata,
        actor: AuditActor,
    ) -> RepositoryResult<JobWithArtifact> {
        self.complete(input, Some(metadata), actor).await
    }

    async fn complete(
        self,
        input: CreatePrintJob,
        metadata: Option<StudioPrintMetadata>,
        actor: AuditActor,
    ) -> RepositoryResult<JobWithArtifact> {
        let created =
            audit::create_print_job_in_transaction(&self.tx, input, metadata, actor).await?;
        self.tx
            .commit()
            .await
            .context("failed to commit artifact quota reservation transaction")?;
        Ok(created)
    }
}
