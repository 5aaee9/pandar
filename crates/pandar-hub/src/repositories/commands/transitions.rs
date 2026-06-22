use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandStatus, TenantId};
use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};

use crate::{db::Database, entities::commands, repositories::RepositoryResult};

pub async fn update_status_if_current(
    database: &Database,
    command_id: CommandId,
    tenant_id: TenantId,
    agent_id: AgentId,
    status: CommandStatus,
    error: Option<String>,
    allowed_statuses: &[CommandStatus],
) -> RepositoryResult<bool> {
    let now = pandar_core::created_at_now();
    let allowed_status_values = allowed_statuses
        .iter()
        .map(|status| status.as_str().to_owned())
        .collect::<Vec<_>>();

    let result = commands::Entity::update_many()
        .set(commands::ActiveModel {
            status: Set(status.as_str().to_owned()),
            error: Set(error),
            updated_at: Set(now),
            ..Default::default()
        })
        .filter(commands::Column::Id.eq(command_id.to_string()))
        .filter(commands::Column::TenantId.eq(tenant_id.to_string()))
        .filter(commands::Column::AgentId.eq(agent_id.to_string()))
        .filter(commands::Column::Status.is_in(allowed_status_values))
        .exec(&database.sea_orm_connection())
        .await
        .context("failed to update command status")?;

    Ok(result.rows_affected == 1)
}
