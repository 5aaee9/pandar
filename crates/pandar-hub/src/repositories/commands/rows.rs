use pandar_core::{AgentId, CommandId, CommandRecord, CommandRecordParts, TenantId};

use crate::{
    entities::commands,
    repositories::{RepositoryError, RepositoryResult},
};

pub fn command_from_model(model: commands::Model) -> RepositoryResult<CommandRecord> {
    let status_for_error = model.status.clone();
    CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::parse(&model.id).map_err(anyhow::Error::from)?,
        tenant_id: TenantId::parse(&model.tenant_id).map_err(anyhow::Error::from)?,
        agent_id: AgentId::parse(&model.agent_id).map_err(anyhow::Error::from)?,
        printer_id: model.printer_id,
        kind: model.kind,
        status: model.status,
        payload_json: model.payload_json,
        error: model.error,
        created_at: model.created_at,
        updated_at: model.updated_at,
    })
    .map_err(|err| match err {
        pandar_core::CoreError::InvalidCommandStatus(_) => {
            RepositoryError::InvalidPersistedCommandStatus(status_for_error)
        }
        err => RepositoryError::Database(
            anyhow::Error::from(err).context("failed to rehydrate command"),
        ),
    })
}
