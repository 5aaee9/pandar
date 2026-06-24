use anyhow::Context;
use pandar_core::{AgentId, JobArtifact, TenantId};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::{
    entities::{job_artifacts, jobs},
    repositories::{JobRepository, RepositoryResult, jobs::rows::artifact_from_model},
};

pub enum AgentArtifactAccess {
    Allowed(JobArtifact),
    Forbidden,
    NotFound,
}

impl JobRepository {
    pub async fn artifact_access_for_agent(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        artifact_id: &str,
    ) -> RepositoryResult<AgentArtifactAccess> {
        let Some(artifact) = job_artifacts::Entity::find_by_id(artifact_id)
            .filter(job_artifacts::Column::TenantId.eq(tenant_id.to_string()))
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to get job artifact for agent")?
        else {
            return Ok(AgentArtifactAccess::NotFound);
        };

        let assigned = jobs::Entity::find()
            .filter(jobs::Column::ArtifactId.eq(artifact_id))
            .filter(jobs::Column::AgentId.eq(agent_id.to_string()))
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to get job for agent artifact")?;
        if assigned.is_none() {
            return Ok(AgentArtifactAccess::Forbidden);
        }

        artifact_from_model(artifact).map(AgentArtifactAccess::Allowed)
    }
}
