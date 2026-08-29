use anyhow::Context;
use pandar_core::{StudioPrintMetadata, TenantId};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    artifacts::lifecycle,
    db::ConnectionDialectExt,
    entities::artifact_quota_reservations,
    repositories::{AuditActor, RepositoryResult},
};

use super::{
    ArtifactQuotaReservation, CreatePrintJob, JobRepository, JobWithArtifact, audit,
    write_transaction,
};

const RESERVATION_TTL: Duration = Duration::minutes(5);

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
        artifact_id: String,
        storage_path: String,
        upload_bytes: u64,
        quota: ArtifactQuotaLimits,
    ) -> RepositoryResult<ArtifactQuotaReservation> {
        let tx = write_transaction::begin(&self.database)
            .await
            .context("failed to begin artifact quota reservation transaction")?;
        lifecycle::lock_artifact_quota(&tx).await?;
        lifecycle::reap_expired_reservations_in_transaction(&tx).await?;
        audit::enforce_artifact_quota(&tx, tenant_id, upload_bytes, quota).await?;
        let now = OffsetDateTime::now_utc();
        let id = uuid::Uuid::new_v4().to_string();
        artifact_quota_reservations::ActiveModel {
            id: Set(id.clone()),
            tenant_id: Set(tenant_id.to_string()),
            artifact_id: Set(artifact_id.clone()),
            storage_path: Set(storage_path.clone()),
            size_bytes: Set(i64::try_from(upload_bytes).context("upload size exceeds int64")?),
            expires_at: Set((now + RESERVATION_TTL)
                .format(&Rfc3339)
                .context("failed to format artifact quota reservation expiry")?),
            created_at: Set(now
                .format(&Rfc3339)
                .context("failed to format artifact quota reservation creation time")?),
        }
        .insert(&tx)
        .await
        .context("failed to insert artifact quota reservation")?;
        tx.commit()
            .await
            .context("failed to commit artifact quota reservation transaction")?;
        Ok(ArtifactQuotaReservation {
            database: self.database.clone(),
            id,
            tenant_id,
            artifact_id,
            storage_path,
        })
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
        &self,
        input: CreatePrintJob,
        actor: AuditActor,
    ) -> RepositoryResult<JobWithArtifact> {
        self.complete(input, None, actor).await
    }

    pub(crate) async fn create_studio_print_job_with_audit(
        &self,
        input: CreatePrintJob,
        metadata: StudioPrintMetadata,
        actor: AuditActor,
    ) -> RepositoryResult<JobWithArtifact> {
        self.complete(input, Some(metadata), actor).await
    }

    async fn complete(
        &self,
        input: CreatePrintJob,
        metadata: Option<StudioPrintMetadata>,
        actor: AuditActor,
    ) -> RepositoryResult<JobWithArtifact> {
        let tx = write_transaction::begin(&self.database)
            .await
            .context("failed to begin artifact quota finalization transaction")?;
        let query = artifact_quota_reservations::Entity::find_by_id(&self.id);
        let reservation = tx
            .lock_for_update(query)
            .one(&tx)
            .await
            .context("failed to lock artifact quota reservation")?
            .ok_or_else(|| anyhow::anyhow!("artifact quota reservation is no longer active"))?;
        if reservation.expires_at <= pandar_core::created_at_now()
            || reservation.tenant_id != self.tenant_id.to_string()
            || reservation.artifact_id != self.artifact_id
            || reservation.storage_path != self.storage_path
            || input.artifact_id != self.artifact_id
            || input.artifact_storage_path != self.storage_path
        {
            return Err(anyhow::anyhow!("artifact quota reservation cannot be finalized").into());
        }
        let created = audit::create_print_job_in_transaction(&tx, input, metadata, actor).await?;
        artifact_quota_reservations::Entity::delete_by_id(&self.id)
            .exec(&tx)
            .await
            .context("failed to finalize artifact quota reservation")?;
        tx.commit()
            .await
            .context("failed to commit artifact quota finalization transaction")?;
        Ok(created)
    }

    pub(crate) async fn release(&self) -> RepositoryResult<()> {
        let tx = write_transaction::begin(&self.database)
            .await
            .context("failed to begin artifact quota release transaction")?;
        let query = artifact_quota_reservations::Entity::find_by_id(&self.id);
        let reservation = tx
            .lock_for_update(query)
            .one(&tx)
            .await
            .context("failed to load artifact quota reservation for release")?;
        if let Some(reservation) = reservation {
            lifecycle::enqueue_deletion(&tx, &reservation.storage_path).await?;
            artifact_quota_reservations::Entity::delete_by_id(&self.id)
                .exec(&tx)
                .await
                .context("failed to release artifact quota reservation")?;
        }
        tx.commit()
            .await
            .context("failed to commit artifact quota release transaction")?;
        Ok(())
    }
}
