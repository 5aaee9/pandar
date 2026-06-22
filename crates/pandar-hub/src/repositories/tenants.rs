use anyhow::Context;
use pandar_core::{Tenant, TenantId};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, DbErr, EntityTrait, PaginatorTrait, QueryOrder, SqlErr,
};

use crate::{
    db::Database,
    entities::tenants,
    repositories::{RepositoryError, RepositoryResult},
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
        let model = tenants::ActiveModel {
            id: Set(tenant.id.to_string()),
            slug: Set(tenant.slug.clone()),
            display_name: Set(tenant.display_name.clone()),
            created_at: Set(tenant.created_at.clone()),
        };
        let result = model
            .insert(&self.database.sea_orm_connection())
            .await
            .map(|_| ());

        match result {
            Ok(_) => Ok(tenant),
            Err(err) if is_duplicate_tenant_slug(&err) => Err(RepositoryError::DuplicateTenantSlug),
            Err(err) => Err(anyhow::Error::new(err)
                .context("failed to insert tenant")
                .into()),
        }
    }

    pub async fn list(&self) -> RepositoryResult<Vec<Tenant>> {
        tenants::Entity::find()
            .order_by_asc(tenants::Column::CreatedAt)
            .order_by_asc(tenants::Column::Id)
            .all(&self.database.sea_orm_connection())
            .await
            .context("failed to list tenants")?
            .into_iter()
            .map(tenant_from_model)
            .collect()
    }

    pub async fn count(&self) -> RepositoryResult<i64> {
        let count = tenants::Entity::find()
            .count(&self.database.sea_orm_connection())
            .await
            .context("failed to count tenants")?;

        Ok(count.try_into().expect("tenant count should fit in i64"))
    }
}

fn tenant_from_model(model: tenants::Model) -> RepositoryResult<Tenant> {
    Tenant::from_parts(
        TenantId::parse(&model.id).map_err(anyhow::Error::from)?,
        model.slug,
        model.display_name,
        model.created_at,
    )
    .map_err(anyhow::Error::from)
    .context("failed to rehydrate tenant")
    .map_err(RepositoryError::from)
}

fn is_duplicate_tenant_slug(err: &DbErr) -> bool {
    let Some(SqlErr::UniqueConstraintViolation(message)) = err.sql_err() else {
        return false;
    };

    message.contains("tenants.slug") || message.contains("tenants_slug_key")
}
