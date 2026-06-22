use anyhow::Context;
use pandar_core::TenantId;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};

use crate::{
    entities::users,
    repositories::{AuthRepository, RepositoryError, RepositoryResult, User, UserRole},
};

mod provisioning;

use super::user_from_model;

impl AuthRepository {
    pub async fn list_users_for_tenant(&self, tenant_id: TenantId) -> RepositoryResult<Vec<User>> {
        users::Entity::find()
            .filter(users::Column::TenantId.eq(tenant_id.to_string()))
            .order_by_asc(users::Column::CreatedAt)
            .order_by_asc(users::Column::Id)
            .all(&self.database.sea_orm_connection())
            .await
            .context("failed to list tenant users")?
            .into_iter()
            .map(user_from_model)
            .collect()
    }

    pub async fn update_user_role(
        &self,
        tenant_id: TenantId,
        user_id: &str,
        role: UserRole,
    ) -> RepositoryResult<User> {
        let connection = self.database.sea_orm_connection();
        update_user_role(&connection, tenant_id, user_id, role).await
    }
}

pub(super) async fn select_user_role<C>(
    connection: &C,
    tenant_id: TenantId,
    user_id: &str,
) -> RepositoryResult<UserRole>
where
    C: sea_orm::ConnectionTrait,
{
    users::Entity::find_by_id(user_id)
        .filter(users::Column::TenantId.eq(tenant_id.to_string()))
        .one(connection)
        .await
        .context("failed to select user role")?
        .map(|user| UserRole::parse(&user.role))
        .transpose()?
        .ok_or(RepositoryError::MissingUser)
}

pub(super) async fn update_user_role<C>(
    connection: &C,
    tenant_id: TenantId,
    user_id: &str,
    role: UserRole,
) -> RepositoryResult<User>
where
    C: sea_orm::ConnectionTrait,
{
    let Some(user) = users::Entity::find_by_id(user_id)
        .filter(users::Column::TenantId.eq(tenant_id.to_string()))
        .one(connection)
        .await
        .context("failed to get user before role update")?
    else {
        return Err(RepositoryError::MissingUser);
    };
    let mut active: users::ActiveModel = user.into();
    active.role = Set(role.as_str().to_owned());
    active
        .update(connection)
        .await
        .context("failed to update user role")
        .map_err(Into::into)
        .and_then(user_from_model)
}
