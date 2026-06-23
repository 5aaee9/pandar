use anyhow::Context;
use pandar_core::{CommandId, CommandStatus, JobId, JobStatus, PrintStatus, TenantId};
use sea_orm::{
    ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, DatabaseTransaction, EntityTrait,
    QueryFilter, TransactionTrait,
};

use crate::{
    db::Database,
    entities::{commands, job_artifacts, job_filament_usages, jobs},
    repositories::{
        AuditActor, DuplicatePrintJob, JobWithArtifact, RepositoryError, RepositoryResult,
        audit::{insert_audit_event_tx, record_audit_event},
        commands::inserts::{self, InsertCommand},
        jobs::{
            create::{self, NewPrintJobFromArtifact},
            job_with_artifact_by_id,
            rows::{job_from_model_with_usage, job_with_artifact_from_models, usage_from_model},
        },
    },
};

pub async fn retry_dispatch_with_audit(
    database: &Database,
    tenant_id: TenantId,
    job_id: JobId,
    reason: Option<String>,
    actor: AuditActor,
) -> RepositoryResult<JobWithArtifact> {
    let connection = database.sea_orm_connection();
    let tx = connection
        .begin()
        .await
        .context("failed to begin retry dispatch transaction")?;
    let source = load_job_for_update(&tx, tenant_id, job_id).await?;
    let command = load_command(&tx, &source).await?;
    if !is_retry_safe(&source, &command) {
        return Err(RepositoryError::RetryNotSafe);
    }

    let command_id = CommandId::new();
    let payload = retry_payload(&command.payload_json, job_id)?;
    inserts::insert(
        &tx,
        InsertCommand {
            id: command_id,
            tenant_id,
            agent_id: source.job.agent_id,
            printer_id: Some(&source.job.printer_id),
            kind: "print_project_file",
            payload_json: &payload,
            created_at: &pandar_core::created_at_now(),
        },
    )
    .await?;
    let update = jobs::Entity::update_many()
        .set(jobs::ActiveModel {
            command_id: Set(command_id.to_string()),
            status: Set(JobStatus::Queued.as_str().to_owned()),
            error: Set(None),
            updated_at: Set(pandar_core::created_at_now()),
            ..Default::default()
        })
        .filter(jobs::Column::TenantId.eq(tenant_id.to_string()))
        .filter(jobs::Column::Id.eq(job_id.to_string()))
        .filter(jobs::Column::CommandId.eq(source.job.command_id.to_string()))
        .filter(jobs::Column::Status.eq(JobStatus::Failed.as_str()))
        .filter(jobs::Column::PrintStatus.eq(PrintStatus::Pending.as_str()))
        .filter(jobs::Column::PrintStartedAt.is_null())
        .filter(
            Condition::any()
                .add(jobs::Column::ProgressPercent.is_null())
                .add(jobs::Column::ProgressPercent.eq(0)),
        )
        .filter(
            Condition::any()
                .add(jobs::Column::CurrentLayer.is_null())
                .add(jobs::Column::CurrentLayer.eq(0)),
        )
        .exec(&tx)
        .await
        .context("failed to update retried print job")?;
    if update.rows_affected != 1 {
        return Err(RepositoryError::RetryNotSafe);
    }
    insert_recovery_audit(
        &tx,
        tenant_id,
        actor,
        RecoveryAudit {
            action: "job.retry_dispatch",
            target_job_id: job_id,
            target_command_id: command_id,
            source_job_id: job_id,
            source_command_id: source.job.command_id,
            reason,
        },
    )
    .await?;
    tx.commit()
        .await
        .context("failed to commit retry dispatch transaction")?;

    job_with_artifact_by_id(database, tenant_id, job_id)
        .await?
        .ok_or(RepositoryError::MissingJob)
}

fn retry_payload(payload_json: &str, job_id: JobId) -> RepositoryResult<String> {
    let mut payload =
        serde_json::from_str::<crate::repositories::PrintProjectFilePayload>(payload_json)
            .context("failed to parse retry source command payload")?;
    payload.job_id = job_id.to_string();
    serde_json::to_string(&payload)
        .context("failed to serialize retry command payload")
        .map_err(RepositoryError::from)
}

pub async fn reprint_with_audit(
    database: &Database,
    tenant_id: TenantId,
    job_id: JobId,
    reason: Option<String>,
    actor: AuditActor,
) -> RepositoryResult<JobWithArtifact> {
    let source = job_with_artifact_by_id(database, tenant_id, job_id)
        .await?
        .ok_or(RepositoryError::MissingJob)?;
    let source_payload = load_source_payload(database, &source).await?;
    if !matches!(
        source.job.print.status,
        PrintStatus::Completed | PrintStatus::Failed | PrintStatus::Cancelled
    ) {
        return Err(RepositoryError::ReprintNotAllowed);
    }

    create_copy_with_audit(
        database,
        CopyJobWithAudit {
            tenant_id,
            source,
            source_payload,
            overrides: None,
            actor,
            action: "job.reprint",
            reason,
        },
    )
    .await
}

pub async fn duplicate_and_print_with_audit(
    database: &Database,
    tenant_id: TenantId,
    job_id: JobId,
    input: DuplicatePrintJob,
    actor: AuditActor,
) -> RepositoryResult<JobWithArtifact> {
    let source = job_with_artifact_by_id(database, tenant_id, job_id)
        .await?
        .ok_or(RepositoryError::MissingJob)?;
    let source_payload = load_source_payload(database, &source).await?;

    create_copy_with_audit(
        database,
        CopyJobWithAudit {
            tenant_id,
            source,
            source_payload,
            overrides: Some(input),
            actor,
            action: "job.duplicate",
            reason: None,
        },
    )
    .await
}

struct CopyJobWithAudit {
    tenant_id: TenantId,
    source: JobWithArtifact,
    source_payload: crate::repositories::PrintProjectFilePayload,
    overrides: Option<DuplicatePrintJob>,
    actor: AuditActor,
    action: &'static str,
    reason: Option<String>,
}

async fn create_copy_with_audit(
    database: &Database,
    input: CopyJobWithAudit,
) -> RepositoryResult<JobWithArtifact> {
    let connection = database.sea_orm_connection();
    let tx = connection
        .begin()
        .await
        .context("failed to begin print recovery transaction")?;
    let source_job_id = input.source.job.id;
    let source_command_id = input.source.job.command_id;
    let created = create::create_print_job_from_artifact(
        &tx,
        NewPrintJobFromArtifact::from_source(input.source, input.source_payload, input.overrides),
    )
    .await?;
    insert_recovery_audit(
        &tx,
        input.tenant_id,
        input.actor,
        RecoveryAudit {
            action: input.action,
            target_job_id: created.job.id,
            target_command_id: created.job.command_id,
            source_job_id,
            source_command_id,
            reason: input.reason,
        },
    )
    .await?;
    tx.commit()
        .await
        .context("failed to commit print recovery transaction")?;
    Ok(created)
}

async fn load_source_payload(
    database: &Database,
    source: &JobWithArtifact,
) -> RepositoryResult<crate::repositories::PrintProjectFilePayload> {
    let command = commands::Entity::find_by_id(source.job.command_id.to_string())
        .one(&database.sea_orm_connection())
        .await
        .context("failed to load recovery source command payload")?
        .ok_or(RepositoryError::MissingCommand)?;
    serde_json::from_str(&command.payload_json)
        .context("failed to parse recovery source command payload")
        .map_err(RepositoryError::from)
}

async fn load_job_for_update<C>(
    connection: &C,
    tenant_id: TenantId,
    job_id: JobId,
) -> RepositoryResult<JobWithArtifact>
where
    C: ConnectionTrait,
{
    let Some(job) = jobs::Entity::find_by_id(job_id.to_string())
        .filter(jobs::Column::TenantId.eq(tenant_id.to_string()))
        .one(connection)
        .await
        .context("failed to load recovery source job")?
    else {
        return Err(RepositoryError::MissingJob);
    };
    let artifact = job_artifacts::Entity::find_by_id(&job.artifact_id)
        .one(connection)
        .await
        .context("failed to load recovery source artifact")?
        .ok_or_else(|| {
            RepositoryError::Database(anyhow::anyhow!(
                "job {} references missing artifact {}",
                job.id,
                job.artifact_id
            ))
        })?;
    let usage = job_filament_usages::Entity::find()
        .filter(job_filament_usages::Column::TenantId.eq(tenant_id.to_string()))
        .filter(job_filament_usages::Column::JobId.eq(&job.id))
        .all(connection)
        .await
        .context("failed to load recovery source filament usage")?
        .into_iter()
        .map(usage_from_model)
        .collect::<RepositoryResult<Vec<_>>>()?;
    let mut source = job_with_artifact_from_models(job.clone(), artifact)?;
    source.job = job_from_model_with_usage(job, usage)?;
    Ok(source)
}

async fn load_command<C>(
    connection: &C,
    source: &JobWithArtifact,
) -> RepositoryResult<pandar_core::CommandRecord>
where
    C: ConnectionTrait,
{
    commands::Entity::find_by_id(source.job.command_id.to_string())
        .one(connection)
        .await
        .context("failed to load retry source command")?
        .map(crate::repositories::commands::rows::command_from_model)
        .transpose()?
        .ok_or(RepositoryError::MissingCommand)
}

fn is_retry_safe(source: &JobWithArtifact, command: &pandar_core::CommandRecord) -> bool {
    source.job.status == JobStatus::Failed
        && command.status == CommandStatus::Failed
        && source.job.print.status == PrintStatus::Pending
        && source.job.print.started_at.is_none()
        && source.job.print.progress_percent.unwrap_or(0) == 0
        && source.job.print.current_layer.unwrap_or(0) == 0
}

struct RecoveryAudit {
    action: &'static str,
    target_job_id: JobId,
    target_command_id: CommandId,
    source_job_id: JobId,
    source_command_id: CommandId,
    reason: Option<String>,
}

async fn insert_recovery_audit(
    connection: &DatabaseTransaction,
    tenant_id: TenantId,
    actor: AuditActor,
    audit: RecoveryAudit,
) -> RepositoryResult<()> {
    let event = record_audit_event(
        tenant_id,
        actor,
        audit.action,
        "job",
        Some(audit.target_job_id.to_string()),
        serde_json::json!({
            "reason": audit.reason,
            "source_job_id": audit.source_job_id,
            "source_command_id": audit.source_command_id,
            "target_job_id": audit.target_job_id,
            "target_command_id": audit.target_command_id,
        }),
    );
    insert_audit_event_tx(connection, &event).await
}
