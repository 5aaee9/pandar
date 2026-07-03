use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandRecord, TenantId};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::{
    entities::commands,
    repositories::{
        CommandRepository, RepositoryError, RepositoryResult, commands::rows::command_from_model,
    },
};

impl CommandRepository {
    pub(crate) async fn load_owned(
        &self,
        command_id: CommandId,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> RepositoryResult<CommandRecord> {
        let command = self
            .get(command_id)
            .await?
            .ok_or(RepositoryError::MissingCommand)?;
        if command.tenant_id != tenant_id || command.agent_id != agent_id {
            return Err(RepositoryError::CommandOwnershipMismatch);
        }

        Ok(command)
    }

    pub async fn get_for_tenant(
        &self,
        tenant_id: TenantId,
        command_id: CommandId,
    ) -> RepositoryResult<Option<CommandRecord>> {
        commands::Entity::find_by_id(command_id.to_string())
            .filter(commands::Column::TenantId.eq(tenant_id.to_string()))
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to load tenant command")?
            .map(command_from_model)
            .transpose()
    }

    pub(super) async fn get(
        &self,
        command_id: CommandId,
    ) -> RepositoryResult<Option<CommandRecord>> {
        commands::Entity::find_by_id(command_id.to_string())
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to load command")?
            .map(command_from_model)
            .transpose()
    }
}
