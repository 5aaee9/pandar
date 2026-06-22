use pandar_core::{AgentId, CommandId, CommandStatus, TenantId};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ConnectionTrait};

use crate::{
    entities::commands,
    repositories::{RepositoryError, RepositoryResult},
};

pub struct InsertCommand<'a> {
    pub id: CommandId,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub printer_id: Option<&'a str>,
    pub kind: &'a str,
    pub payload_json: &'a str,
    pub created_at: &'a str,
}

pub async fn insert<C>(connection: &C, input: InsertCommand<'_>) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    commands::ActiveModel {
        id: Set(input.id.to_string()),
        tenant_id: Set(input.tenant_id.to_string()),
        agent_id: Set(input.agent_id.to_string()),
        printer_id: Set(input.printer_id.map(str::to_owned)),
        kind: Set(input.kind.to_owned()),
        status: Set(CommandStatus::Queued.as_str().to_owned()),
        payload_json: Set(input.payload_json.to_owned()),
        result_json: Set(None),
        error: Set(None),
        created_at: Set(input.created_at.to_owned()),
        updated_at: Set(input.created_at.to_owned()),
    }
    .insert(connection)
    .await
    .map(|_| ())
    .map_err(|err| {
        RepositoryError::Database(anyhow::Error::new(err).context("failed to insert command"))
    })
}
