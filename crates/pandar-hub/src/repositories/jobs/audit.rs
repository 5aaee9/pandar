use anyhow::Context;
use pandar_core::StudioPrintMetadata;
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement, TryGetable};

use crate::{
    db::Database,
    repositories::{
        AuditActor, CreatePrintJob, JobWithArtifact, RepositoryResult,
        audit::{EmptyAuditMetadata, audit_metadata, insert_audit_event_tx, record_audit_event},
        jobs::{ArtifactQuotaLimits, create, write_transaction},
    },
};

pub async fn create_print_job_with_audit(
    database: &Database,
    input: CreatePrintJob,
    actor: AuditActor,
) -> RepositoryResult<JobWithArtifact> {
    create_print_job_with_optional_metadata(database, input, None, None, actor).await
}

pub async fn create_print_job_with_quota_and_audit(
    database: &Database,
    input: CreatePrintJob,
    quota: ArtifactQuotaLimits,
    actor: AuditActor,
) -> RepositoryResult<JobWithArtifact> {
    create_print_job_with_optional_metadata(database, input, None, Some(quota), actor).await
}

pub async fn create_studio_print_job_with_audit(
    database: &Database,
    input: CreatePrintJob,
    metadata: StudioPrintMetadata,
    actor: AuditActor,
) -> RepositoryResult<JobWithArtifact> {
    create_print_job_with_optional_metadata(database, input, Some(metadata), None, actor).await
}

pub async fn create_studio_print_job_with_quota_and_audit(
    database: &Database,
    input: CreatePrintJob,
    metadata: StudioPrintMetadata,
    quota: ArtifactQuotaLimits,
    actor: AuditActor,
) -> RepositoryResult<JobWithArtifact> {
    create_print_job_with_optional_metadata(database, input, Some(metadata), Some(quota), actor)
        .await
}

async fn create_print_job_with_optional_metadata(
    database: &Database,
    input: CreatePrintJob,
    metadata: Option<StudioPrintMetadata>,
    quota: Option<ArtifactQuotaLimits>,
    actor: AuditActor,
) -> RepositoryResult<JobWithArtifact> {
    let tx = write_transaction::begin(database)
        .await
        .context("failed to begin print job audit transaction")?;
    if let Some(quota) = quota {
        enforce_artifact_quota(&tx, input.tenant_id, input.artifact.size_bytes, quota).await?;
    }
    let created = create_print_job_in_transaction(&tx, input, metadata, actor).await?;
    tx.commit()
        .await
        .context("failed to commit print job audit transaction")?;
    Ok(created)
}

pub(super) async fn create_print_job_in_transaction(
    tx: &sea_orm::DatabaseTransaction,
    input: CreatePrintJob,
    metadata: Option<StudioPrintMetadata>,
    actor: AuditActor,
) -> RepositoryResult<JobWithArtifact> {
    let created = create::create_print_job(tx, input, metadata).await?;
    let event = record_audit_event(
        created.job.tenant_id,
        actor,
        "job.create",
        "job",
        Some(created.job.id.to_string()),
        audit_metadata(EmptyAuditMetadata {}),
    );
    insert_audit_event_tx(tx, &event).await?;
    Ok(created)
}

pub(super) async fn enforce_artifact_quota(
    tx: &sea_orm::DatabaseTransaction,
    tenant_id: pandar_core::TenantId,
    upload_bytes: u64,
    quota: ArtifactQuotaLimits,
) -> RepositoryResult<()> {
    let backend = tx.get_database_backend();
    let tenant_id = tenant_id.to_string();
    if backend == DatabaseBackend::Postgres {
        tx.execute_raw(Statement::from_string(
            backend,
            "SELECT pg_advisory_xact_lock(1886596974)".to_owned(),
        ))
        .await
        .context("failed to lock PostgreSQL global artifact quota")?;
        tx.execute_raw(Statement::from_sql_and_values(
            backend,
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
            [tenant_id.clone().into()],
        ))
        .await
        .context("failed to lock PostgreSQL tenant artifact quota")?;
    }
    let now = pandar_core::created_at_now();
    let sql = match backend {
        DatabaseBackend::Postgres => {
            "SELECT (COALESCE((SELECT SUM(size_bytes) FROM job_artifacts WHERE tenant_id = $1), 0) + COALESCE((SELECT SUM(size_bytes) FROM artifact_quota_reservations WHERE tenant_id = $1 AND expires_at > $2), 0))::BIGINT AS bytes, ((SELECT COUNT(*) FROM job_artifacts WHERE tenant_id = $1) + (SELECT COUNT(*) FROM artifact_quota_reservations WHERE tenant_id = $1 AND expires_at > $2))::BIGINT AS count"
        }
        _ => {
            "SELECT COALESCE((SELECT SUM(size_bytes) FROM job_artifacts WHERE tenant_id = ?1), 0) + COALESCE((SELECT SUM(size_bytes) FROM artifact_quota_reservations WHERE tenant_id = ?1 AND expires_at > ?2), 0) AS bytes, (SELECT COUNT(*) FROM job_artifacts WHERE tenant_id = ?1) + (SELECT COUNT(*) FROM artifact_quota_reservations WHERE tenant_id = ?1 AND expires_at > ?2) AS count"
        }
    };
    let usage = tx
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            sql,
            [tenant_id.into(), now.clone().into()],
        ))
        .await
        .context("failed to load transactional tenant artifact usage")?
        .expect("artifact usage aggregate always returns one row");
    let bytes = i64::try_get(&usage, "", "bytes")
        .map_err(|err| anyhow::anyhow!("failed to decode tenant artifact bytes: {err:?}"))?;
    let count = i64::try_get(&usage, "", "count")
        .map_err(|err| anyhow::anyhow!("failed to decode tenant artifact count: {err:?}"))?;
    let bytes = u64::try_from(bytes).context("tenant artifact bytes must be non-negative")?;
    let count = u64::try_from(count).context("tenant artifact count must be non-negative")?;
    if count >= quota.tenant_count || bytes.saturating_add(upload_bytes) > quota.tenant_bytes {
        return Err(crate::repositories::RepositoryError::ArtifactQuotaExceeded);
    }
    let global_usage = tx
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            match backend {
                DatabaseBackend::Postgres => "SELECT (COALESCE((SELECT SUM(size_bytes) FROM job_artifacts), 0) + COALESCE((SELECT SUM(size_bytes) FROM artifact_quota_reservations WHERE expires_at > $1), 0))::BIGINT AS bytes, ((SELECT COUNT(*) FROM job_artifacts) + (SELECT COUNT(*) FROM artifact_quota_reservations WHERE expires_at > $1))::BIGINT AS count",
                _ => "SELECT COALESCE((SELECT SUM(size_bytes) FROM job_artifacts), 0) + COALESCE((SELECT SUM(size_bytes) FROM artifact_quota_reservations WHERE expires_at > ?1), 0) AS bytes, (SELECT COUNT(*) FROM job_artifacts) + (SELECT COUNT(*) FROM artifact_quota_reservations WHERE expires_at > ?1) AS count",
            },
            [now.into()],
        ))
        .await
        .context("failed to load transactional global artifact usage")?
        .expect("global artifact usage aggregate always returns one row");
    let global_bytes = u64::try_from(
        i64::try_get(&global_usage, "", "bytes")
            .map_err(|err| anyhow::anyhow!("failed to decode global artifact bytes: {err:?}"))?,
    )
    .context("global artifact bytes must be non-negative")?;
    let global_count = u64::try_from(
        i64::try_get(&global_usage, "", "count")
            .map_err(|err| anyhow::anyhow!("failed to decode global artifact count: {err:?}"))?,
    )
    .context("global artifact count must be non-negative")?;
    if global_count >= quota.global_count
        || global_bytes.saturating_add(upload_bytes) > quota.global_bytes
    {
        return Err(crate::repositories::RepositoryError::ArtifactQuotaExceeded);
    }
    Ok(())
}
