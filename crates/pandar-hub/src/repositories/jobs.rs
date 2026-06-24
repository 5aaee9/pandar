use anyhow::Context;
use pandar_core::{
    AgentId, CommandId, CommandRecord, CommandStatus, Job, JobArtifact, JobId, JobStatus, TenantId,
};
#[cfg(test)]
use sea_orm::PaginatorTrait;
use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, TransactionTrait};

mod artifacts;
mod audit;
mod create;
pub(crate) mod hydration;
mod print_reports;
mod recovery;
pub mod rows;
mod transitions;

use crate::{
    db::Database,
    entities::jobs,
    repositories::{AuditActor, RepositoryResult},
};

pub use artifacts::AgentArtifactAccess;
pub use print_reports::{AppliedPrintReport, ApplyPrintReport, PrintReportDiagnostic};
use rows::job_from_model_loading_usage;

#[derive(Debug, Clone)]
pub struct JobRepository {
    database: Database,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatePrintJob {
    pub tenant_id: TenantId,
    pub printer_id: String,
    pub agent_id: AgentId,
    pub artifact_id: String,
    pub artifact_filename: String,
    pub artifact_content_type: String,
    pub artifact_size_bytes: u64,
    pub artifact_storage_path: String,
    pub artifact_metadata_json: Option<String>,
    pub plate_id: u32,
    pub use_ams: bool,
    pub flow_cali: bool,
    pub timelapse: bool,
    pub ams_mapping_json: Option<String>,
    pub ams_mapping2_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicatePrintJob {
    pub printer_id: Option<String>,
    pub plate_id: Option<u32>,
    pub use_ams: Option<bool>,
    pub flow_cali: Option<bool>,
    pub timelapse: Option<bool>,
    pub ams_mapping_json: Option<String>,
    pub ams_mapping2_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobWithArtifact {
    pub job: Job,
    pub artifact: JobArtifact,
}

impl JobRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn create_print_job(
        &self,
        input: CreatePrintJob,
    ) -> RepositoryResult<JobWithArtifact> {
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin print job transaction")?;
        let created = create::create_print_job(&tx, input).await?;
        tx.commit()
            .await
            .context("failed to commit print job transaction")?;
        Ok(created)
    }

    pub async fn create_print_job_with_audit(
        &self,
        input: CreatePrintJob,
        actor: AuditActor,
    ) -> RepositoryResult<JobWithArtifact> {
        audit::create_print_job_with_audit(&self.database, input, actor).await
    }

    pub async fn retry_dispatch_with_audit(
        &self,
        tenant_id: TenantId,
        job_id: JobId,
        reason: Option<String>,
        actor: AuditActor,
    ) -> RepositoryResult<JobWithArtifact> {
        recovery::retry_dispatch_with_audit(&self.database, tenant_id, job_id, reason, actor).await
    }

    pub async fn reprint_with_audit(
        &self,
        tenant_id: TenantId,
        job_id: JobId,
        reason: Option<String>,
        actor: AuditActor,
    ) -> RepositoryResult<JobWithArtifact> {
        recovery::reprint_with_audit(&self.database, tenant_id, job_id, reason, actor).await
    }

    pub async fn duplicate_and_print_with_audit(
        &self,
        tenant_id: TenantId,
        job_id: JobId,
        input: DuplicatePrintJob,
        actor: AuditActor,
    ) -> RepositoryResult<JobWithArtifact> {
        recovery::duplicate_and_print_with_audit(&self.database, tenant_id, job_id, input, actor)
            .await
    }

    pub async fn mark_for_command(
        &self,
        command_id: CommandId,
        status: JobStatus,
        error: Option<String>,
    ) -> RepositoryResult<Option<Job>> {
        let updated = jobs::Entity::update_many()
            .set(jobs::ActiveModel {
                status: Set(status.as_str().to_owned()),
                error: Set(error),
                updated_at: Set(pandar_core::created_at_now()),
                ..Default::default()
            })
            .filter(jobs::Column::CommandId.eq(command_id.to_string()))
            .filter(jobs::Column::Status.is_not_in(["succeeded", "failed"]))
            .exec(&self.database.sea_orm_connection())
            .await
            .context("failed to update job for command")?
            .rows_affected;

        if updated == 0 && self.get_by_command(command_id).await?.is_none() {
            return Ok(None);
        }

        self.get_by_command(command_id).await
    }

    pub async fn mark_print_sent(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<CommandRecord> {
        self.transition_print_command(transitions::PrintCommandTransition {
            command_id,
            tenant_id,
            agent_id,
            command_status: CommandStatus::Sent,
            job_status: JobStatus::Sent,
            error: None,
            allowed_statuses: &[CommandStatus::Queued],
            action: "send",
        })
        .await
    }

    pub async fn mark_print_acknowledged(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<CommandRecord> {
        self.transition_print_command(transitions::PrintCommandTransition {
            command_id,
            tenant_id,
            agent_id,
            command_status: CommandStatus::Acknowledged,
            job_status: JobStatus::Acknowledged,
            error: None,
            allowed_statuses: &[CommandStatus::Sent],
            action: "acknowledge",
        })
        .await
    }

    pub async fn mark_print_failed(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
        error: String,
    ) -> RepositoryResult<CommandRecord> {
        self.transition_print_command(transitions::PrintCommandTransition {
            command_id,
            tenant_id,
            agent_id,
            command_status: CommandStatus::Failed,
            job_status: JobStatus::Failed,
            error: Some(error),
            allowed_statuses: &[CommandStatus::Sent, CommandStatus::Acknowledged],
            action: "fail",
        })
        .await
    }

    pub async fn mark_print_succeeded(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<CommandRecord> {
        self.transition_print_command(transitions::PrintCommandTransition {
            command_id,
            tenant_id,
            agent_id,
            command_status: CommandStatus::Succeeded,
            job_status: JobStatus::Succeeded,
            error: None,
            allowed_statuses: &[CommandStatus::Sent, CommandStatus::Acknowledged],
            action: "succeed",
        })
        .await
    }

    async fn transition_print_command(
        &self,
        transition: transitions::PrintCommandTransition<'_>,
    ) -> RepositoryResult<CommandRecord> {
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin print command transition transaction")?;
        let command = transitions::transition_print_command(&tx, transition).await?;
        tx.commit()
            .await
            .context("failed to commit print command transition")?;
        Ok(command)
    }

    async fn get_by_command(&self, command_id: CommandId) -> RepositoryResult<Option<Job>> {
        let Some(job) = jobs::Entity::find()
            .filter(jobs::Column::CommandId.eq(command_id.to_string()))
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to get job by command")?
        else {
            return Ok(None);
        };

        job_from_model_loading_usage(&self.database.sea_orm_connection(), job)
            .await
            .map(Some)
    }

    #[cfg(test)]
    pub(crate) async fn filament_usage_count(
        &self,
        tenant_id: TenantId,
        job_id: JobId,
    ) -> RepositoryResult<i64> {
        let count = crate::entities::job_filament_usages::Entity::find()
            .filter(
                crate::entities::job_filament_usages::Column::TenantId.eq(tenant_id.to_string()),
            )
            .filter(crate::entities::job_filament_usages::Column::JobId.eq(job_id.to_string()))
            .count(&self.database.sea_orm_connection())
            .await
            .context("failed to count job filament usage")?;

        Ok(count
            .try_into()
            .expect("filament usage count should fit in i64"))
    }

    pub async fn apply_print_report(
        &self,
        input: ApplyPrintReport,
    ) -> RepositoryResult<AppliedPrintReport> {
        print_reports::apply_print_report(&self.database, input).await
    }
}
