use anyhow::Context;
use sea_orm::TransactionTrait;

use crate::{
    db::Database,
    repositories::{
        CreatePrintJob, JobWithArtifact, RecordAuditEvent, RepositoryResult,
        audit::{build_audit_event, insert_audit_event_tx},
        jobs::create,
    },
};

pub async fn create_print_job_with_audit(
    database: &Database,
    input: CreatePrintJob,
    user_id: String,
) -> RepositoryResult<JobWithArtifact> {
    let connection = database.sea_orm_connection();
    let tx = connection
        .begin()
        .await
        .context("failed to begin print job audit transaction")?;
    let created = create::create_print_job(&tx, input).await?;
    let event = build_audit_event(RecordAuditEvent {
        tenant_id: created.job.tenant_id,
        actor_type: "user".to_owned(),
        user_id: Some(user_id),
        action: "job.create".to_owned(),
        target_type: "job".to_owned(),
        target_id: Some(created.job.id.to_string()),
        metadata_json: "{}".to_owned(),
    });
    insert_audit_event_tx(&tx, &event).await?;
    tx.commit()
        .await
        .context("failed to commit print job audit transaction")?;
    Ok(created)
}
