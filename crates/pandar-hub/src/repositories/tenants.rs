use anyhow::Context;
use pandar_core::{Tenant, TenantId};
use sqlx::Row;

use crate::{
    db::Database,
    repositories::{RepositoryError, RepositoryResult, is_unique_violation},
};

#[derive(Debug, Clone)]
pub struct TenantRepository {
    database: Database,
}

impl TenantRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn create(
        &self,
        slug: impl Into<String>,
        display_name: impl Into<String>,
    ) -> RepositoryResult<Tenant> {
        let tenant = Tenant::new(slug, display_name).map_err(anyhow::Error::from)?;
        let result = match &self.database {
            Database::Sqlite(pool) => sqlx::query(
                "INSERT INTO tenants (id, slug, display_name, created_at) VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(tenant.id.to_string())
            .bind(&tenant.slug)
            .bind(&tenant.display_name)
            .bind(&tenant.created_at)
            .execute(pool)
            .await
            .map(|_| ()),
            Database::Postgres(pool) => sqlx::query(
                "INSERT INTO tenants (id, slug, display_name, created_at) VALUES ($1, $2, $3, $4)",
            )
            .bind(tenant.id.to_string())
            .bind(&tenant.slug)
            .bind(&tenant.display_name)
            .bind(&tenant.created_at)
            .execute(pool)
            .await
            .map(|_| ()),
        };

        match result {
            Ok(_) => Ok(tenant),
            Err(err) if is_unique_violation(&err, "tenants.slug", "tenants_slug_key") => {
                Err(RepositoryError::DuplicateTenantSlug)
            }
            Err(err) => Err(anyhow::Error::new(err)
                .context("failed to insert tenant")
                .into()),
        }
    }

    pub async fn list(&self) -> RepositoryResult<Vec<Tenant>> {
        match &self.database {
            Database::Sqlite(pool) => {
                let rows = sqlx::query(
                    "SELECT id, slug, display_name, created_at FROM tenants ORDER BY created_at ASC, id ASC",
                )
                .fetch_all(pool)
                .await
                .context("failed to list SQLite tenants")?;
                rows.into_iter()
                    .map(|row| {
                        tenant_from_parts(
                            row.get("id"),
                            row.get("slug"),
                            row.get("display_name"),
                            row.get("created_at"),
                        )
                    })
                    .collect()
            }
            Database::Postgres(pool) => {
                let rows = sqlx::query(
                    "SELECT id, slug, display_name, created_at FROM tenants ORDER BY created_at ASC, id ASC",
                )
                .fetch_all(pool)
                .await
                .context("failed to list PostgreSQL tenants")?;
                rows.into_iter()
                    .map(|row| {
                        tenant_from_parts(
                            row.get("id"),
                            row.get("slug"),
                            row.get("display_name"),
                            row.get("created_at"),
                        )
                    })
                    .collect()
            }
        }
    }

    pub async fn count(&self) -> RepositoryResult<i64> {
        let count = match &self.database {
            Database::Sqlite(pool) => {
                sqlx::query_scalar("SELECT COUNT(*) FROM tenants")
                    .fetch_one(pool)
                    .await
            }
            Database::Postgres(pool) => {
                sqlx::query_scalar("SELECT COUNT(*) FROM tenants")
                    .fetch_one(pool)
                    .await
            }
        }
        .context("failed to count tenants")?;

        Ok(count)
    }
}

fn tenant_from_parts(
    id: String,
    slug: String,
    display_name: String,
    created_at: String,
) -> RepositoryResult<Tenant> {
    Tenant::from_parts(
        TenantId::parse(&id).map_err(anyhow::Error::from)?,
        slug,
        display_name,
        created_at,
    )
    .map_err(anyhow::Error::from)
    .context("failed to rehydrate tenant")
    .map_err(RepositoryError::from)
}
