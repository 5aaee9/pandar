use sea_orm::DatabaseTransaction;
use serde::Serialize;

use super::{ClearJobsOutcome, ClearScope};
use crate::{
    entities::{job_artifacts, jobs},
    repositories::{
        AuditActor, RepositoryResult,
        audit::{audit_metadata, insert_audit_event_tx, record_audit_event},
    },
};

#[derive(Serialize)]
pub(super) struct DeleteJobAuditContext {
    job_id: String,
    artifact_id: String,
    artifact_filename: String,
    printer_id: String,
    agent_id: String,
    command_id: String,
    previous_dispatch_status: String,
    previous_print_status: String,
}

impl DeleteJobAuditContext {
    pub(super) fn from_models(job: &jobs::Model, artifact: &job_artifacts::Model) -> Self {
        Self {
            job_id: job.id.clone(),
            artifact_id: job.artifact_id.clone(),
            artifact_filename: artifact.filename.clone(),
            printer_id: job.printer_id.clone(),
            agent_id: job.agent_id.clone(),
            command_id: job.command_id.clone(),
            previous_dispatch_status: job.status.clone(),
            previous_print_status: job.print_status.clone(),
        }
    }
}

#[derive(Serialize)]
struct ClearJobsAuditMetadata {
    deleted_jobs: u64,
    retained_jobs: u64,
    deleted_commands: u64,
    deleted_artifacts: u64,
    deleted_artifact_bytes: u64,
}

#[derive(Serialize)]
struct DeleteJobAuditMetadata<'a> {
    #[serde(flatten)]
    context: &'a DeleteJobAuditContext,
    deleted_jobs: u64,
    retained_jobs: u64,
    deleted_commands: u64,
    deleted_artifacts: u64,
    deleted_artifact_bytes: u64,
}

pub(super) async fn insert_clear_audit(
    tx: &DatabaseTransaction,
    tenant_id: pandar_core::TenantId,
    actor: AuditActor,
    scope: &ClearScope,
    delete_context: Option<&DeleteJobAuditContext>,
    outcome: &ClearJobsOutcome,
) -> RepositoryResult<()> {
    let (action, target_type, target_id, metadata) = match scope {
        ClearScope::Tenant => (
            "job.clear",
            "job_collection",
            None,
            audit_metadata(ClearJobsAuditMetadata {
                deleted_jobs: outcome.deleted_jobs,
                retained_jobs: outcome.retained_jobs,
                deleted_commands: outcome.deleted_commands,
                deleted_artifacts: outcome.deleted_artifacts,
                deleted_artifact_bytes: outcome.deleted_artifact_bytes,
            }),
        ),
        ClearScope::Job(job_id) => (
            "job.delete",
            "job",
            Some(job_id.clone()),
            audit_metadata(DeleteJobAuditMetadata {
                context: delete_context.expect("single-job audit context exists"),
                deleted_jobs: outcome.deleted_jobs,
                retained_jobs: outcome.retained_jobs,
                deleted_commands: outcome.deleted_commands,
                deleted_artifacts: outcome.deleted_artifacts,
                deleted_artifact_bytes: outcome.deleted_artifact_bytes,
            }),
        ),
    };
    let event = record_audit_event(tenant_id, actor, action, target_type, target_id, metadata);
    insert_audit_event_tx(tx, &event).await
}
