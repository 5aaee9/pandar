use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandRecord, CommandStatus, JobStatus, TenantId};
use sea_orm::TransactionTrait;

use super::{JobRepository, transitions};
use crate::repositories::RepositoryResult;

impl JobRepository {
    #[cfg(test)]
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
            result_json: None,
            allowed_statuses: &[CommandStatus::Queued],
            action: "send",
        })
        .await
    }

    #[cfg(test)]
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
            result_json: None,
            allowed_statuses: &[CommandStatus::Sent],
            action: "acknowledge",
        })
        .await
    }

    #[cfg(test)]
    pub async fn mark_print_failed(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
        error: String,
    ) -> RepositoryResult<CommandRecord> {
        self.mark_print_failed_with_result(command_id, tenant_id, agent_id, error, None)
            .await
    }

    #[cfg(test)]
    pub async fn mark_print_failed_with_result(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
        error: String,
        result_json: Option<String>,
    ) -> RepositoryResult<CommandRecord> {
        self.transition_print_command(transitions::PrintCommandTransition {
            command_id,
            tenant_id,
            agent_id,
            command_status: CommandStatus::Failed,
            job_status: JobStatus::Failed,
            error: Some(error),
            result_json,
            allowed_statuses: &[CommandStatus::Sent, CommandStatus::Acknowledged],
            action: "fail",
        })
        .await
    }

    #[cfg(test)]
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
            result_json: None,
            allowed_statuses: &[CommandStatus::Sent, CommandStatus::Acknowledged],
            action: "succeed",
        })
        .await
    }

    #[cfg(test)]
    pub async fn mark_print_succeeded_with_result(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
        result_json: Option<String>,
    ) -> RepositoryResult<CommandRecord> {
        self.transition_print_command(transitions::PrintCommandTransition {
            command_id,
            tenant_id,
            agent_id,
            command_status: CommandStatus::Succeeded,
            job_status: JobStatus::Succeeded,
            error: None,
            result_json,
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
}
