use anyhow::Context;
use pandar_core::{CommandStatus, JobStatus, PrintStatus, StudioSubmissionId, TenantId};
use sea_orm::{
    ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseTransaction, EntityTrait, QueryFilter,
    QuerySelect,
};

use crate::{
    db::Database,
    entities::{commands, jobs},
    repositories::{
        AuditActor, JobWithArtifact, RepositoryError, RepositoryResult,
        audit::{EmptyAuditMetadata, audit_metadata, insert_audit_event_tx, record_audit_event},
        jobs::{hydration::job_with_artifact_by_id, write_transaction},
    },
};

pub async fn cancel_studio_print_with_audit(
    database: &Database,
    tenant_id: TenantId,
    studio_submission_id: StudioSubmissionId,
    actor: AuditActor,
) -> RepositoryResult<JobWithArtifact> {
    let tx = write_transaction::begin(database)
        .await
        .context("failed to begin Studio print cancellation transaction")?;
    let job_reference = load_job_reference(&tx, tenant_id, studio_submission_id).await?;
    let command = load_command(&tx, tenant_id, &job_reference.command_id).await?;
    let job = load_job_for_update(&tx, tenant_id, studio_submission_id).await?;
    if command.status == CommandStatus::Cancelled.as_str()
        && job.status == JobStatus::Cancelled.as_str()
        && job.print_status == PrintStatus::Cancelled.as_str()
    {
        tx.commit()
            .await
            .context("failed to commit idempotent Studio print cancellation")?;
        return load_cancelled(database, tenant_id, &job.id).await;
    }
    if command.status != CommandStatus::Queued.as_str() {
        return Err(RepositoryError::StudioCancellationTooLate);
    }

    let now = pandar_core::created_at_now();
    let command_update = commands::Entity::update_many()
        .set(commands::ActiveModel {
            status: Set(CommandStatus::Cancelled.as_str().to_owned()),
            error: Set(None),
            updated_at: Set(now.clone()),
            ..Default::default()
        })
        .filter(commands::Column::Id.eq(job.command_id.clone()))
        .filter(commands::Column::TenantId.eq(tenant_id.to_string()))
        .filter(commands::Column::Kind.eq("print_project_file"))
        .filter(commands::Column::Status.eq(CommandStatus::Queued.as_str()))
        .exec(&tx)
        .await
        .context("failed to cancel queued Studio print command")?;
    if command_update.rows_affected != 1 {
        return Err(RepositoryError::StudioCancellationTooLate);
    }

    let job_update = jobs::Entity::update_many()
        .set(jobs::ActiveModel {
            status: Set(JobStatus::Cancelled.as_str().to_owned()),
            print_status: Set(PrintStatus::Cancelled.as_str().to_owned()),
            error: Set(None),
            updated_at: Set(now),
            ..Default::default()
        })
        .filter(jobs::Column::Id.eq(job.id.clone()))
        .filter(jobs::Column::TenantId.eq(tenant_id.to_string()))
        .filter(jobs::Column::CommandId.eq(job.command_id))
        .filter(jobs::Column::Status.eq(JobStatus::Queued.as_str()))
        .filter(jobs::Column::PrintStatus.eq(PrintStatus::Pending.as_str()))
        .exec(&tx)
        .await
        .context("failed to cancel queued Studio print job")?;
    if job_update.rows_affected != 1 {
        return Err(RepositoryError::StudioCancellationTooLate);
    }

    let job_id = pandar_core::JobId::parse(&job.id)
        .map_err(anyhow::Error::from)
        .context("failed to parse cancelled Studio print job id")?;
    let event = record_audit_event(
        tenant_id,
        actor,
        "job.cancel",
        "job",
        Some(job.id.clone()),
        audit_metadata(EmptyAuditMetadata {}),
    );
    insert_audit_event_tx(&tx, &event).await?;
    tx.commit()
        .await
        .context("failed to commit Studio print cancellation")?;
    job_with_artifact_by_id(database, tenant_id, job_id)
        .await?
        .ok_or(RepositoryError::MissingJob)
}

async fn load_job_reference(
    tx: &DatabaseTransaction,
    tenant_id: TenantId,
    studio_submission_id: StudioSubmissionId,
) -> RepositoryResult<jobs::Model> {
    jobs::Entity::find()
        .filter(jobs::Column::TenantId.eq(tenant_id.to_string()))
        .filter(jobs::Column::StudioSubmissionId.eq(studio_submission_id.get()))
        .one(tx)
        .await
        .context("failed to locate Studio print for cancellation")?
        .ok_or(RepositoryError::MissingJob)
}

async fn load_job_for_update(
    tx: &DatabaseTransaction,
    tenant_id: TenantId,
    studio_submission_id: StudioSubmissionId,
) -> RepositoryResult<jobs::Model> {
    let query = jobs::Entity::find()
        .filter(jobs::Column::TenantId.eq(tenant_id.to_string()))
        .filter(jobs::Column::StudioSubmissionId.eq(studio_submission_id.get()));
    let job = match tx.get_database_backend() {
        sea_orm::DatabaseBackend::Postgres => query.lock_exclusive().one(tx).await,
        _ => query.one(tx).await,
    }
    .context("failed to load Studio print for cancellation")?;
    job.ok_or(RepositoryError::MissingJob)
}

async fn load_command(
    tx: &DatabaseTransaction,
    tenant_id: TenantId,
    command_id: &str,
) -> RepositoryResult<commands::Model> {
    let query = commands::Entity::find_by_id(command_id)
        .filter(commands::Column::TenantId.eq(tenant_id.to_string()))
        .filter(commands::Column::Kind.eq("print_project_file"));
    let command = match tx.get_database_backend() {
        sea_orm::DatabaseBackend::Postgres => query.lock_exclusive().one(tx).await,
        _ => query.one(tx).await,
    }
    .context("failed to load Studio print command for cancellation")?;
    command.ok_or(RepositoryError::MissingCommand)
}

async fn load_cancelled(
    database: &Database,
    tenant_id: TenantId,
    job_id: &str,
) -> RepositoryResult<JobWithArtifact> {
    let job_id = pandar_core::JobId::parse(job_id)
        .map_err(anyhow::Error::from)
        .context("failed to parse idempotently cancelled Studio print job id")?;
    job_with_artifact_by_id(database, tenant_id, job_id)
        .await?
        .ok_or(RepositoryError::MissingJob)
}
