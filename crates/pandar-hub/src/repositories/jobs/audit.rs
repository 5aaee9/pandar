use anyhow::Context;
use sea_orm::TransactionTrait;

use crate::{
    db::Database,
    repositories::{
        AuditActor, CreatePrintJob, JobWithArtifact, RepositoryResult,
        audit::{insert_audit_event_tx, record_audit_event},
        jobs::create,
    },
};

pub async fn create_print_job_with_audit(
    database: &Database,
    input: CreatePrintJob,
    actor: AuditActor,
) -> RepositoryResult<JobWithArtifact> {
    let connection = database.sea_orm_connection();
    let tx = connection
        .begin()
        .await
        .context("failed to begin print job audit transaction")?;
    let created = create::create_print_job(&tx, input).await?;
    let event = record_audit_event(
        created.job.tenant_id,
        actor,
        "job.create",
        "job",
        Some(created.job.id.to_string()),
        serde_json::json!({}),
    );
    insert_audit_event_tx(&tx, &event).await?;
    tx.commit()
        .await
        .context("failed to commit print job audit transaction")?;
    Ok(created)
}
