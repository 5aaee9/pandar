use anyhow::Context;

use crate::{
    db::Database,
    repositories::{
        CreatePrintJob, JobWithArtifact, RecordAuditEvent, RepositoryResult,
        audit::{build_audit_event, insert_audit_event_postgres, insert_audit_event_sqlite},
        jobs::create::{create_print_job_postgres, create_print_job_sqlite},
    },
};

pub async fn create_print_job_with_audit(
    database: &Database,
    input: CreatePrintJob,
    user_id: String,
) -> RepositoryResult<JobWithArtifact> {
    match database {
        Database::Sqlite(pool) => {
            let mut transaction = pool
                .begin()
                .await
                .context("failed to begin SQLite print job audit transaction")?;
            let created = create_print_job_sqlite(&mut transaction, input).await;
            match created {
                Ok(created) => {
                    let event = build_audit_event(RecordAuditEvent {
                        tenant_id: created.job.tenant_id,
                        actor_type: "user".to_owned(),
                        user_id: Some(user_id),
                        action: "job.create".to_owned(),
                        target_type: "job".to_owned(),
                        target_id: Some(created.job.id.to_string()),
                        metadata_json: "{}".to_owned(),
                    });
                    insert_audit_event_sqlite(&mut *transaction, &event).await?;
                    transaction
                        .commit()
                        .await
                        .context("failed to commit SQLite print job audit transaction")?;
                    Ok(created)
                }
                Err(err) => {
                    transaction
                        .rollback()
                        .await
                        .context("failed to roll back SQLite print job audit transaction")?;
                    Err(err)
                }
            }
        }
        Database::Postgres(pool) => {
            let mut transaction = pool
                .begin()
                .await
                .context("failed to begin PostgreSQL print job audit transaction")?;
            let created = create_print_job_postgres(&mut transaction, input).await;
            match created {
                Ok(created) => {
                    let event = build_audit_event(RecordAuditEvent {
                        tenant_id: created.job.tenant_id,
                        actor_type: "user".to_owned(),
                        user_id: Some(user_id),
                        action: "job.create".to_owned(),
                        target_type: "job".to_owned(),
                        target_id: Some(created.job.id.to_string()),
                        metadata_json: "{}".to_owned(),
                    });
                    insert_audit_event_postgres(&mut *transaction, &event).await?;
                    transaction
                        .commit()
                        .await
                        .context("failed to commit PostgreSQL print job audit transaction")?;
                    Ok(created)
                }
                Err(err) => {
                    transaction
                        .rollback()
                        .await
                        .context("failed to roll back PostgreSQL print job audit transaction")?;
                    Err(err)
                }
            }
        }
    }
}
