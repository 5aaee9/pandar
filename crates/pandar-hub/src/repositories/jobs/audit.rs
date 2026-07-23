use anyhow::Context;
use pandar_core::StudioPrintMetadata;

use crate::{
    db::Database,
    repositories::{
        AuditActor, CreatePrintJob, JobWithArtifact, RepositoryResult,
        audit::{EmptyAuditMetadata, audit_metadata, insert_audit_event_tx, record_audit_event},
        jobs::{create, write_transaction},
    },
};

pub async fn create_print_job_with_audit(
    database: &Database,
    input: CreatePrintJob,
    actor: AuditActor,
) -> RepositoryResult<JobWithArtifact> {
    create_print_job_with_optional_metadata(database, input, None, actor).await
}

pub async fn create_studio_print_job_with_audit(
    database: &Database,
    input: CreatePrintJob,
    metadata: StudioPrintMetadata,
    actor: AuditActor,
) -> RepositoryResult<JobWithArtifact> {
    create_print_job_with_optional_metadata(database, input, Some(metadata), actor).await
}

async fn create_print_job_with_optional_metadata(
    database: &Database,
    input: CreatePrintJob,
    metadata: Option<StudioPrintMetadata>,
    actor: AuditActor,
) -> RepositoryResult<JobWithArtifact> {
    let tx = write_transaction::begin(database)
        .await
        .context("failed to begin print job audit transaction")?;
    let created = create::create_print_job(&tx, input, metadata).await?;
    let event = record_audit_event(
        created.job.tenant_id,
        actor,
        "job.create",
        "job",
        Some(created.job.id.to_string()),
        audit_metadata(EmptyAuditMetadata {}),
    );
    insert_audit_event_tx(&tx, &event).await?;
    tx.commit()
        .await
        .context("failed to commit print job audit transaction")?;
    Ok(created)
}
