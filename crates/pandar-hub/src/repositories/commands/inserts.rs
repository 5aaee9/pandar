use pandar_core::{AgentId, CommandId, CommandStatus, TenantId};

use crate::{
    db::Database,
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

pub async fn insert(database: &Database, input: InsertCommand<'_>) -> RepositoryResult<()> {
    match database {
        Database::Sqlite(pool) => insert_sqlite(pool, input).await,
        Database::Postgres(pool) => insert_postgres(pool, input).await,
    }
}

pub async fn insert_sqlite<'e, E>(executor: E, input: InsertCommand<'_>) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query(
        "INSERT INTO commands (id, tenant_id, agent_id, printer_id, kind, status, payload_json, error, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9)",
    )
    .bind(input.id.to_string())
    .bind(input.tenant_id.to_string())
    .bind(input.agent_id.to_string())
    .bind(input.printer_id)
    .bind(input.kind)
    .bind(CommandStatus::Queued.as_str())
    .bind(input.payload_json)
    .bind(input.created_at)
    .bind(input.created_at)
    .execute(executor)
    .await
    .map(|_| ())
    .map_err(insert_error)
}

pub async fn insert_postgres<'e, E>(executor: E, input: InsertCommand<'_>) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query(
        "INSERT INTO commands (id, tenant_id, agent_id, printer_id, kind, status, payload_json, error, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, $8, $9)",
    )
    .bind(input.id.to_string())
    .bind(input.tenant_id.to_string())
    .bind(input.agent_id.to_string())
    .bind(input.printer_id)
    .bind(input.kind)
    .bind(CommandStatus::Queued.as_str())
    .bind(input.payload_json)
    .bind(input.created_at)
    .bind(input.created_at)
    .execute(executor)
    .await
    .map(|_| ())
    .map_err(insert_error)
}

fn insert_error(err: sqlx::Error) -> RepositoryError {
    RepositoryError::Database(anyhow::Error::new(err).context("failed to insert command"))
}
