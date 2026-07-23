use anyhow::Context;
use pandar_core::{StudioSubmissionId, TenantId};
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

use crate::repositories::{RepositoryError, RepositoryResult};

pub(super) async fn allocate<C>(
    connection: &C,
    tenant_id: TenantId,
) -> RepositoryResult<StudioSubmissionId>
where
    C: ConnectionTrait,
{
    let backend = connection.get_database_backend();
    let tenant = tenant_id.to_string();
    let insert_sql = match backend {
        DatabaseBackend::Sqlite => {
            "INSERT INTO studio_submission_sequences (tenant_id, last_id) VALUES (?1, 0) ON CONFLICT (tenant_id) DO NOTHING"
        }
        DatabaseBackend::Postgres => {
            "INSERT INTO studio_submission_sequences (tenant_id, last_id) VALUES ($1, 0) ON CONFLICT (tenant_id) DO NOTHING"
        }
        other => unreachable!("unsupported database backend for Studio id allocation: {other:?}"),
    };
    connection
        .execute_raw(Statement::from_sql_and_values(
            backend,
            insert_sql,
            [tenant.clone().into()],
        ))
        .await
        .context("failed to initialize Studio submission sequence")?;

    let update_sql = match backend {
        DatabaseBackend::Sqlite => {
            "UPDATE studio_submission_sequences SET last_id = last_id + 1 WHERE tenant_id = ?1 AND last_id < 2147483647 RETURNING last_id"
        }
        DatabaseBackend::Postgres => {
            "UPDATE studio_submission_sequences SET last_id = last_id + 1 WHERE tenant_id = $1 AND last_id < 2147483647 RETURNING last_id"
        }
        other => unreachable!("unsupported database backend for Studio id allocation: {other:?}"),
    };
    let Some(row) = connection
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            update_sql,
            [tenant.into()],
        ))
        .await
        .context("failed to allocate Studio submission id")?
    else {
        return Err(RepositoryError::StudioSubmissionIdExhausted);
    };
    let value = row
        .try_get::<i32>("", "last_id")
        .context("failed to decode allocated Studio submission id")?;
    StudioSubmissionId::try_from(i64::from(value))
        .map_err(anyhow::Error::from)
        .context("failed to validate allocated Studio submission id")
        .map_err(RepositoryError::from)
}
