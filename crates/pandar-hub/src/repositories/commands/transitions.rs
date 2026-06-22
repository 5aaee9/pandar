use anyhow::Context;
use pandar_core::{AgentId, CommandId, CommandStatus, TenantId};
use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};

use crate::{db::Database, entities::commands, repositories::RepositoryResult};

pub struct StatusTransition<'a> {
    pub command_id: CommandId,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub status: CommandStatus,
    pub error: Option<String>,
    pub result_json: Option<String>,
    pub allowed_statuses: &'a [CommandStatus],
}

pub async fn update_status_if_current(
    database: &Database,
    transition: StatusTransition<'_>,
) -> RepositoryResult<bool> {
    let now = pandar_core::created_at_now();
    let allowed_status_values = transition
        .allowed_statuses
        .iter()
        .map(|status| status.as_str().to_owned())
        .collect::<Vec<_>>();

    let result = commands::Entity::update_many()
        .set(commands::ActiveModel {
            status: Set(transition.status.as_str().to_owned()),
            error: Set(transition.error),
            result_json: Set(transition.result_json),
            updated_at: Set(now),
            ..Default::default()
        })
        .filter(commands::Column::Id.eq(transition.command_id.to_string()))
        .filter(commands::Column::TenantId.eq(transition.tenant_id.to_string()))
        .filter(commands::Column::AgentId.eq(transition.agent_id.to_string()))
        .filter(commands::Column::Status.is_in(allowed_status_values))
        .exec(&database.sea_orm_connection())
        .await
        .context("failed to update command status")?;

    Ok(result.rows_affected == 1)
}
