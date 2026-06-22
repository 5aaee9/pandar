use anyhow::Context;
use pandar_core::TenantId;
use sqlx::Row;

use crate::{
    db::Database,
    repositories::{AuthRepository, RepositoryError, RepositoryResult, User, UserRole},
};

mod provisioning;

use super::user_from_row;

impl AuthRepository {
    pub async fn list_users_for_tenant(&self, tenant_id: TenantId) -> RepositoryResult<Vec<User>> {
        match &self.database {
            Database::Sqlite(pool) => {
                let rows = sqlx::query(
                    "SELECT id, tenant_id, email, display_name, role, created_at
                     FROM users
                     WHERE tenant_id = ?1
                     ORDER BY created_at ASC, id ASC",
                )
                .bind(tenant_id.to_string())
                .fetch_all(pool)
                .await
                .context("failed to list SQLite tenant users")?;
                rows.into_iter()
                    .map(|row| {
                        user_from_row(
                            row.get("id"),
                            row.get("tenant_id"),
                            row.get("email"),
                            row.get("display_name"),
                            row.get("role"),
                            row.get("created_at"),
                        )
                    })
                    .collect()
            }
            Database::Postgres(pool) => {
                let rows = sqlx::query(
                    "SELECT id, tenant_id, email, display_name, role, created_at
                     FROM users
                     WHERE tenant_id = $1
                     ORDER BY created_at ASC, id ASC",
                )
                .bind(tenant_id.to_string())
                .fetch_all(pool)
                .await
                .context("failed to list PostgreSQL tenant users")?;
                rows.into_iter()
                    .map(|row| {
                        user_from_row(
                            row.get("id"),
                            row.get("tenant_id"),
                            row.get("email"),
                            row.get("display_name"),
                            row.get("role"),
                            row.get("created_at"),
                        )
                    })
                    .collect()
            }
        }
    }

    pub async fn update_user_role(
        &self,
        tenant_id: TenantId,
        user_id: &str,
        role: UserRole,
    ) -> RepositoryResult<User> {
        match &self.database {
            Database::Sqlite(pool) => sqlx::query(
                "UPDATE users
                 SET role = ?1
                 WHERE tenant_id = ?2 AND id = ?3
                 RETURNING id, tenant_id, email, display_name, role, created_at",
            )
            .bind(role.as_str())
            .bind(tenant_id.to_string())
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .context("failed to update SQLite user role")?
            .map(|row| {
                user_from_row(
                    row.get("id"),
                    row.get("tenant_id"),
                    row.get("email"),
                    row.get("display_name"),
                    row.get("role"),
                    row.get("created_at"),
                )
            })
            .transpose()?
            .ok_or(RepositoryError::MissingUser),
            Database::Postgres(pool) => sqlx::query(
                "UPDATE users
                 SET role = $1
                 WHERE tenant_id = $2 AND id = $3
                 RETURNING id, tenant_id, email, display_name, role, created_at",
            )
            .bind(role.as_str())
            .bind(tenant_id.to_string())
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .context("failed to update PostgreSQL user role")?
            .map(|row| {
                user_from_row(
                    row.get("id"),
                    row.get("tenant_id"),
                    row.get("email"),
                    row.get("display_name"),
                    row.get("role"),
                    row.get("created_at"),
                )
            })
            .transpose()?
            .ok_or(RepositoryError::MissingUser),
        }
    }
}
