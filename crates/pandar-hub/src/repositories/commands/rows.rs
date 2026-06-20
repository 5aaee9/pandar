use pandar_core::{AgentId, CommandId, CommandRecord, CommandRecordParts, TenantId};
use sqlx::Row;

use crate::repositories::{RepositoryError, RepositoryResult};

pub fn command_from_row<R>(row: R) -> RepositoryResult<CommandRecord>
where
    R: Row,
    String: for<'r> sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    Option<String>: for<'r> sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    for<'a> &'a str: sqlx::ColumnIndex<R>,
{
    command_from_parts(CommandRowParts {
        id: row.get("id"),
        tenant_id: row.get("tenant_id"),
        agent_id: row.get("agent_id"),
        printer_id: row.get("printer_id"),
        kind: row.get("kind"),
        status: row.get("status"),
        payload_json: row.get("payload_json"),
        error: row.get("error"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

struct CommandRowParts {
    id: String,
    tenant_id: String,
    agent_id: String,
    printer_id: Option<String>,
    kind: String,
    status: String,
    payload_json: String,
    error: Option<String>,
    created_at: String,
    updated_at: String,
}

fn command_from_parts(parts: CommandRowParts) -> RepositoryResult<CommandRecord> {
    let status_for_error = parts.status.clone();
    CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::parse(&parts.id).map_err(anyhow::Error::from)?,
        tenant_id: TenantId::parse(&parts.tenant_id).map_err(anyhow::Error::from)?,
        agent_id: AgentId::parse(&parts.agent_id).map_err(anyhow::Error::from)?,
        printer_id: parts.printer_id,
        kind: parts.kind,
        status: parts.status,
        payload_json: parts.payload_json,
        error: parts.error,
        created_at: parts.created_at,
        updated_at: parts.updated_at,
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
