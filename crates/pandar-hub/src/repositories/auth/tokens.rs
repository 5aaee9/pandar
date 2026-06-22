use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use sqlx::Row;

use crate::{
    db::Database,
    repositories::{ApiToken, AuthRepository, RepositoryError, RepositoryResult},
};

mod provisioning;

impl AuthRepository {
    pub async fn list_api_tokens_for_user(
        &self,
        tenant_id: TenantId,
        user_id: &str,
    ) -> RepositoryResult<Vec<ApiToken>> {
        match &self.database {
            Database::Sqlite(pool) => {
                ensure_user_exists_sqlite(pool, tenant_id, user_id).await?;
                let rows = sqlx::query(
                    "SELECT id, tenant_id, user_id, name, created_at, last_used_at, revoked_at
                     FROM api_tokens
                     WHERE tenant_id = ?1 AND user_id = ?2
                     ORDER BY created_at ASC, id ASC",
                )
                .bind(tenant_id.to_string())
                .bind(user_id)
                .fetch_all(pool)
                .await
                .context("failed to list SQLite user api tokens")?;
                rows.into_iter()
                    .map(|row| {
                        api_token_from_parts(
                            row.get("id"),
                            row.get("tenant_id"),
                            row.get("user_id"),
                            row.get("name"),
                            row.get("created_at"),
                            row.get("last_used_at"),
                            row.get("revoked_at"),
                        )
                    })
                    .collect()
            }
            Database::Postgres(pool) => {
                ensure_user_exists_postgres(pool, tenant_id, user_id).await?;
                let rows = sqlx::query(
                    "SELECT id, tenant_id, user_id, name, created_at, last_used_at, revoked_at
                     FROM api_tokens
                     WHERE tenant_id = $1 AND user_id = $2
                     ORDER BY created_at ASC, id ASC",
                )
                .bind(tenant_id.to_string())
                .bind(user_id)
                .fetch_all(pool)
                .await
                .context("failed to list PostgreSQL user api tokens")?;
                rows.into_iter()
                    .map(|row| {
                        api_token_from_parts(
                            row.get("id"),
                            row.get("tenant_id"),
                            row.get("user_id"),
                            row.get("name"),
                            row.get("created_at"),
                            row.get("last_used_at"),
                            row.get("revoked_at"),
                        )
                    })
                    .collect()
            }
        }
    }

    pub async fn revoke_api_token(
        &self,
        tenant_id: TenantId,
        token_id: &str,
    ) -> RepositoryResult<ApiToken> {
        let token = self.get_api_token(tenant_id, token_id).await?;
        let Some(token) = token else {
            return Err(RepositoryError::MissingApiToken);
        };
        if token.revoked_at.is_some() {
            return Ok(token);
        }

        let revoked_at = created_at_now();
        match &self.database {
            Database::Sqlite(pool) => {
                let row = sqlx::query(
                    "UPDATE api_tokens
                     SET revoked_at = ?1
                     WHERE tenant_id = ?2 AND id = ?3
                     RETURNING id, tenant_id, user_id, name, created_at, last_used_at, revoked_at",
                )
                .bind(&revoked_at)
                .bind(tenant_id.to_string())
                .bind(token_id)
                .fetch_one(pool)
                .await
                .context("failed to revoke SQLite api token")?;
                api_token_from_parts(
                    row.get("id"),
                    row.get("tenant_id"),
                    row.get("user_id"),
                    row.get("name"),
                    row.get("created_at"),
                    row.get("last_used_at"),
                    row.get("revoked_at"),
                )
            }
            Database::Postgres(pool) => {
                let row = sqlx::query(
                    "UPDATE api_tokens
                     SET revoked_at = $1
                     WHERE tenant_id = $2 AND id = $3
                     RETURNING id, tenant_id, user_id, name, created_at, last_used_at, revoked_at",
                )
                .bind(&revoked_at)
                .bind(tenant_id.to_string())
                .bind(token_id)
                .fetch_one(pool)
                .await
                .context("failed to revoke PostgreSQL api token")?;
                api_token_from_parts(
                    row.get("id"),
                    row.get("tenant_id"),
                    row.get("user_id"),
                    row.get("name"),
                    row.get("created_at"),
                    row.get("last_used_at"),
                    row.get("revoked_at"),
                )
            }
        }
    }

    async fn get_api_token(
        &self,
        tenant_id: TenantId,
        token_id: &str,
    ) -> RepositoryResult<Option<ApiToken>> {
        match &self.database {
            Database::Sqlite(pool) => {
                let row = sqlx::query(
                    "SELECT id, tenant_id, user_id, name, created_at, last_used_at, revoked_at
                     FROM api_tokens
                     WHERE tenant_id = ?1 AND id = ?2",
                )
                .bind(tenant_id.to_string())
                .bind(token_id)
                .fetch_optional(pool)
                .await
                .context("failed to get SQLite api token")?;
                row.map(|row| {
                    api_token_from_parts(
                        row.get("id"),
                        row.get("tenant_id"),
                        row.get("user_id"),
                        row.get("name"),
                        row.get("created_at"),
                        row.get("last_used_at"),
                        row.get("revoked_at"),
                    )
                })
                .transpose()
            }
            Database::Postgres(pool) => {
                let row = sqlx::query(
                    "SELECT id, tenant_id, user_id, name, created_at, last_used_at, revoked_at
                     FROM api_tokens
                     WHERE tenant_id = $1 AND id = $2",
                )
                .bind(tenant_id.to_string())
                .bind(token_id)
                .fetch_optional(pool)
                .await
                .context("failed to get PostgreSQL api token")?;
                row.map(|row| {
                    api_token_from_parts(
                        row.get("id"),
                        row.get("tenant_id"),
                        row.get("user_id"),
                        row.get("name"),
                        row.get("created_at"),
                        row.get("last_used_at"),
                        row.get("revoked_at"),
                    )
                })
                .transpose()
            }
        }
    }
}

async fn ensure_user_exists_sqlite<'e, E>(
    executor: E,
    tenant_id: TenantId,
    user_id: &str,
) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let exists =
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM users WHERE tenant_id = ?1 AND id = ?2")
            .bind(tenant_id.to_string())
            .bind(user_id)
            .fetch_optional(executor)
            .await
            .context("failed to check SQLite api token owner")?;
    exists.map(|_| ()).ok_or(RepositoryError::MissingUser)
}

async fn ensure_user_exists_postgres<'e, E>(
    executor: E,
    tenant_id: TenantId,
    user_id: &str,
) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let exists =
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM users WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id.to_string())
            .bind(user_id)
            .fetch_optional(executor)
            .await
            .context("failed to check PostgreSQL api token owner")?;
    exists.map(|_| ()).ok_or(RepositoryError::MissingUser)
}

pub(super) fn api_token_from_parts(
    id: String,
    tenant_id: String,
    user_id: String,
    name: String,
    created_at: String,
    last_used_at: Option<String>,
    revoked_at: Option<String>,
) -> RepositoryResult<ApiToken> {
    Ok(ApiToken {
        id,
        tenant_id: TenantId::parse(&tenant_id).map_err(anyhow::Error::from)?,
        user_id,
        name,
        created_at,
        last_used_at,
        revoked_at,
    })
}
