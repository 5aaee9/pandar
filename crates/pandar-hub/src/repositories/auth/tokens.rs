use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder,
};

use crate::{
    entities::api_tokens,
    repositories::{
        ApiToken, AuthRepository, RepositoryError, RepositoryResult, auth::ensure_user_exists,
    },
};

mod provisioning;

impl AuthRepository {
    pub async fn list_api_tokens_for_user(
        &self,
        tenant_id: TenantId,
        user_id: &str,
    ) -> RepositoryResult<Vec<ApiToken>> {
        let connection = self.database.sea_orm_connection();
        ensure_user_exists(
            &connection,
            tenant_id,
            user_id,
            "failed to check api token owner",
        )
        .await?;

        api_tokens::Entity::find()
            .filter(api_tokens::Column::TenantId.eq(tenant_id.to_string()))
            .filter(api_tokens::Column::UserId.eq(user_id))
            .order_by_asc(api_tokens::Column::CreatedAt)
            .order_by_asc(api_tokens::Column::Id)
            .all(&connection)
            .await
            .context("failed to list user api tokens")?
            .into_iter()
            .map(api_token_from_model)
            .collect()
    }

    pub async fn revoke_api_token(
        &self,
        tenant_id: TenantId,
        token_id: &str,
    ) -> RepositoryResult<ApiToken> {
        let connection = self.database.sea_orm_connection();
        revoke_api_token(&connection, tenant_id, token_id).await
    }
}

pub(super) async fn revoke_api_token<C>(
    connection: &C,
    tenant_id: TenantId,
    token_id: &str,
) -> RepositoryResult<ApiToken>
where
    C: ConnectionTrait,
{
    let Some(token) = api_tokens::Entity::find_by_id(token_id)
        .filter(api_tokens::Column::TenantId.eq(tenant_id.to_string()))
        .one(connection)
        .await
        .context("failed to get api token before revoke")?
    else {
        return Err(RepositoryError::MissingApiToken);
    };

    if token.revoked_at.is_some() {
        return api_token_from_model(token);
    }

    let mut active: api_tokens::ActiveModel = token.into();
    active.revoked_at = Set(Some(created_at_now()));
    active
        .update(connection)
        .await
        .context("failed to revoke api token")
        .map_err(Into::into)
        .and_then(api_token_from_model)
}

pub(super) fn api_token_from_model(model: api_tokens::Model) -> RepositoryResult<ApiToken> {
    api_token_from_parts(
        model.id,
        model.tenant_id,
        model.user_id,
        model.name,
        model.created_at,
        model.last_used_at,
        model.revoked_at,
    )
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
