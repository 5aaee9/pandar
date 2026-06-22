use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder,
};

use crate::{
    entities::{user_identities, users},
    repositories::{
        AuthRepository, AuthenticatedUser, RepositoryError, RepositoryResult,
        auth::{authenticated_from_models, ensure_user_exists},
        is_sea_orm_foreign_key_violation, is_sea_orm_unique_violation,
    },
};

mod provisioning;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserIdentity {
    pub id: String,
    pub tenant_id: TenantId,
    pub user_id: String,
    pub provider: String,
    pub subject: String,
    pub created_at: String,
}

impl AuthRepository {
    pub async fn link_external_identity(
        &self,
        tenant_id: TenantId,
        user_id: &str,
        provider: impl Into<String>,
        subject: impl Into<String>,
    ) -> RepositoryResult<UserIdentity> {
        let identity = UserIdentity {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id,
            user_id: user_id.to_owned(),
            provider: provider.into(),
            subject: subject.into(),
            created_at: created_at_now(),
        };

        insert_identity(
            &self.database.sea_orm_connection(),
            &identity,
            "failed to insert external identity",
        )
        .await?;
        Ok(identity)
    }

    pub async fn authenticate_external_identity(
        &self,
        tenant_id: TenantId,
        provider: &str,
        subject: &str,
    ) -> RepositoryResult<Option<AuthenticatedUser>> {
        let connection = self.database.sea_orm_connection();
        let Some(identity) = user_identities::Entity::find()
            .filter(user_identities::Column::TenantId.eq(tenant_id.to_string()))
            .filter(user_identities::Column::Provider.eq(provider))
            .filter(user_identities::Column::Subject.eq(subject))
            .one(&connection)
            .await
            .context("failed to authenticate external identity")?
        else {
            return Ok(None);
        };
        let Some(user) = users::Entity::find_by_id(identity.user_id.clone())
            .filter(users::Column::TenantId.eq(identity.tenant_id.clone()))
            .one(&connection)
            .await
            .context("failed to load external identity user")?
        else {
            return Ok(None);
        };
        authenticated_from_models(identity_token(identity), user).map(Some)
    }

    pub async fn list_external_identities_for_user(
        &self,
        tenant_id: TenantId,
        user_id: &str,
    ) -> RepositoryResult<Vec<UserIdentity>> {
        let connection = self.database.sea_orm_connection();
        ensure_user_exists(
            &connection,
            tenant_id,
            user_id,
            "failed to check user identity owner",
        )
        .await?;

        user_identities::Entity::find()
            .filter(user_identities::Column::TenantId.eq(tenant_id.to_string()))
            .filter(user_identities::Column::UserId.eq(user_id))
            .order_by_asc(user_identities::Column::CreatedAt)
            .order_by_asc(user_identities::Column::Id)
            .all(&connection)
            .await
            .context("failed to list user external identities")?
            .into_iter()
            .map(user_identity_from_model)
            .collect()
    }
}

pub(super) async fn insert_identity<C>(
    connection: &C,
    identity: &UserIdentity,
    context: &'static str,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    ensure_user_exists(
        connection,
        identity.tenant_id,
        &identity.user_id,
        "failed to check user identity owner",
    )
    .await?;

    let result = identity_model(identity)
        .insert(connection)
        .await
        .map(|_| ());
    match result {
        Ok(()) => Ok(()),
        Err(err)
            if is_sea_orm_unique_violation(
                &err,
                USER_IDENTITIES_EXTERNAL_UNIQUE_SQLITE,
                USER_IDENTITIES_EXTERNAL_UNIQUE_POSTGRES,
            ) || is_sea_orm_unique_violation(
                &err,
                USER_IDENTITIES_USER_PROVIDER_UNIQUE_SQLITE,
                USER_IDENTITIES_USER_PROVIDER_UNIQUE_POSTGRES,
            ) =>
        {
            if external_identity_exists(connection, identity).await? {
                Err(RepositoryError::DuplicateExternalIdentity)
            } else {
                Err(RepositoryError::DuplicateUserExternalIdentity)
            }
        }
        Err(err) if is_sea_orm_foreign_key_violation(&err) => Err(RepositoryError::MissingUser),
        Err(err) => Err(anyhow::Error::new(err).context(context).into()),
    }
}

async fn external_identity_exists<C>(
    connection: &C,
    identity: &UserIdentity,
) -> RepositoryResult<bool>
where
    C: ConnectionTrait,
{
    user_identities::Entity::find()
        .filter(user_identities::Column::TenantId.eq(identity.tenant_id.to_string()))
        .filter(user_identities::Column::Provider.eq(identity.provider.clone()))
        .filter(user_identities::Column::Subject.eq(identity.subject.clone()))
        .one(connection)
        .await
        .context("failed to inspect duplicate external identity")
        .map(|identity| identity.is_some())
        .map_err(Into::into)
}

fn user_identity_from_model(model: user_identities::Model) -> RepositoryResult<UserIdentity> {
    Ok(UserIdentity {
        id: model.id,
        tenant_id: TenantId::parse(&model.tenant_id).map_err(anyhow::Error::from)?,
        user_id: model.user_id,
        provider: model.provider,
        subject: model.subject,
        created_at: model.created_at,
    })
}

fn identity_model(identity: &UserIdentity) -> user_identities::ActiveModel {
    user_identities::ActiveModel {
        id: Set(identity.id.clone()),
        tenant_id: Set(identity.tenant_id.to_string()),
        user_id: Set(identity.user_id.clone()),
        provider: Set(identity.provider.clone()),
        subject: Set(identity.subject.clone()),
        created_at: Set(identity.created_at.clone()),
    }
}

fn identity_token(identity: user_identities::Model) -> crate::entities::api_tokens::Model {
    crate::entities::api_tokens::Model {
        id: identity.id,
        tenant_id: identity.tenant_id,
        user_id: identity.user_id,
        name: identity.provider,
        token_hash: identity.subject,
        created_at: identity.created_at,
        last_used_at: None,
        revoked_at: None,
    }
}

pub(super) const USER_IDENTITIES_EXTERNAL_UNIQUE_SQLITE: &str =
    "user_identities.tenant_id, user_identities.provider, user_identities.subject";
pub(super) const USER_IDENTITIES_EXTERNAL_UNIQUE_POSTGRES: &str =
    "user_identities_tenant_id_provider_subject_key";
pub(super) const USER_IDENTITIES_USER_PROVIDER_UNIQUE_SQLITE: &str =
    "user_identities.tenant_id, user_identities.user_id, user_identities.provider";
pub(super) const USER_IDENTITIES_USER_PROVIDER_UNIQUE_POSTGRES: &str =
    "user_identities_tenant_id_user_id_provider_key";
