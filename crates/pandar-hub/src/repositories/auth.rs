use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    db::Database,
    entities::{api_tokens, users as user_entities},
    repositories::{
        RepositoryError, RepositoryResult, is_sea_orm_foreign_key_violation,
        is_sea_orm_unique_violation,
    },
};

mod bootstrap;
mod identities;
mod tokens;
mod users;

pub use identities::UserIdentity;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum UserRole {
    TenantAdmin,
    Operator,
    Viewer,
}

impl UserRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TenantAdmin => "tenant_admin",
            Self::Operator => "operator",
            Self::Viewer => "viewer",
        }
    }

    pub fn parse(value: &str) -> RepositoryResult<Self> {
        match value {
            "tenant_admin" => Ok(Self::TenantAdmin),
            "operator" => Ok(Self::Operator),
            "viewer" => Ok(Self::Viewer),
            other => Err(RepositoryError::InvalidPersistedUserRole(other.to_owned())),
        }
    }

    pub fn allows(self, required: Self) -> bool {
        self.rank() >= required.rank()
    }

    fn rank(self) -> u8 {
        match self {
            Self::Viewer => 0,
            Self::Operator => 1,
            Self::TenantAdmin => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct User {
    pub id: String,
    pub tenant_id: TenantId,
    pub email: String,
    pub display_name: String,
    pub role: UserRole,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiToken {
    pub id: String,
    pub tenant_id: TenantId,
    pub user_id: String,
    pub name: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser {
    pub token_id: String,
    pub user: User,
}

#[derive(Debug, Clone)]
pub struct AuthRepository {
    database: Database,
}

impl AuthRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn create_user(
        &self,
        tenant_id: TenantId,
        email: impl Into<String>,
        display_name: impl Into<String>,
        role: UserRole,
    ) -> RepositoryResult<User> {
        let user = User {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id,
            email: email.into(),
            display_name: display_name.into(),
            role,
            created_at: created_at_now(),
        };

        insert_user(
            &self.database.sea_orm_connection(),
            &user,
            "failed to insert user",
        )
        .await?;
        Ok(user)
    }

    pub async fn create_api_token(
        &self,
        tenant_id: TenantId,
        user_id: &str,
        name: impl Into<String>,
        plaintext_token: &str,
    ) -> RepositoryResult<ApiToken> {
        let token = ApiToken {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id,
            user_id: user_id.to_owned(),
            name: name.into(),
            created_at: created_at_now(),
            last_used_at: None,
            revoked_at: None,
        };
        let token_hash = hash_token(plaintext_token);

        insert_api_token(
            &self.database.sea_orm_connection(),
            &token,
            &token_hash,
            "failed to insert api token",
        )
        .await?;
        Ok(token)
    }

    pub async fn authenticate_bearer(
        &self,
        plaintext_token: &str,
    ) -> RepositoryResult<Option<AuthenticatedUser>> {
        let token_hash = hash_token(plaintext_token);
        let connection = self.database.sea_orm_connection();
        let Some(token) = api_tokens::Entity::find()
            .filter(api_tokens::Column::TokenHash.eq(token_hash))
            .filter(api_tokens::Column::RevokedAt.is_null())
            .one(&connection)
            .await
            .context("failed to authenticate bearer token")?
        else {
            return Ok(None);
        };
        let Some(user) = user_entities::Entity::find_by_id(token.user_id.clone())
            .filter(user_entities::Column::TenantId.eq(token.tenant_id.clone()))
            .one(&connection)
            .await
            .context("failed to load authenticated bearer user")?
        else {
            return Ok(None);
        };
        let mut active: api_tokens::ActiveModel = token.clone().into();
        active.last_used_at = Set(Some(created_at_now()));
        active
            .update(&connection)
            .await
            .context("failed to update bearer token last_used_at")?;

        authenticated_from_models(token, user).map(Some)
    }
}

pub(super) fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    format!("{digest:x}")
}

pub(super) fn user_from_row(
    id: String,
    tenant_id: String,
    email: String,
    display_name: String,
    role: String,
    created_at: String,
) -> RepositoryResult<User> {
    Ok(User {
        id,
        tenant_id: TenantId::parse(&tenant_id).map_err(anyhow::Error::from)?,
        email,
        display_name,
        role: UserRole::parse(&role)?,
        created_at,
    })
}

pub(super) fn user_from_model(model: user_entities::Model) -> RepositoryResult<User> {
    user_from_row(
        model.id,
        model.tenant_id,
        model.email,
        model.display_name,
        model.role,
        model.created_at,
    )
}

pub(super) async fn insert_user<C>(
    connection: &C,
    user: &User,
    context: &'static str,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    let result = user_model(user).insert(connection).await.map(|_| ());
    match result {
        Ok(()) => Ok(()),
        Err(err)
            if is_sea_orm_unique_violation(
                &err,
                "users.tenant_id, users.email",
                "users_tenant_id_email_key",
            ) =>
        {
            Err(RepositoryError::DuplicateUserEmail)
        }
        Err(err) if is_sea_orm_foreign_key_violation(&err) => Err(RepositoryError::MissingTenant),
        Err(err) => Err(anyhow::Error::new(err).context(context).into()),
    }
}

pub(super) async fn user_exists<C>(
    connection: &C,
    tenant_id: TenantId,
    user_id: &str,
    context: &'static str,
) -> RepositoryResult<bool>
where
    C: ConnectionTrait,
{
    user_entities::Entity::find_by_id(user_id)
        .filter(user_entities::Column::TenantId.eq(tenant_id.to_string()))
        .one(connection)
        .await
        .context(context)
        .map(|user| user.is_some())
        .map_err(Into::into)
}

pub(super) async fn ensure_user_exists<C>(
    connection: &C,
    tenant_id: TenantId,
    user_id: &str,
    context: &'static str,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    user_exists(connection, tenant_id, user_id, context)
        .await?
        .then_some(())
        .ok_or(RepositoryError::MissingUser)
}

pub(super) async fn insert_api_token<C>(
    connection: &C,
    token: &ApiToken,
    token_hash: &str,
    context: &'static str,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    ensure_user_exists(
        connection,
        token.tenant_id,
        &token.user_id,
        "failed to check api token owner",
    )
    .await?;

    let result = api_token_model(token, token_hash)
        .insert(connection)
        .await
        .map(|_| ());
    match result {
        Ok(()) => Ok(()),
        Err(err)
            if is_sea_orm_unique_violation(
                &err,
                "api_tokens.tenant_id, api_tokens.name",
                "api_tokens_tenant_id_name_key",
            ) =>
        {
            Err(RepositoryError::DuplicateApiTokenName)
        }
        Err(err)
            if is_sea_orm_unique_violation(
                &err,
                "api_tokens.token_hash",
                "api_tokens_token_hash_key",
            ) =>
        {
            Err(RepositoryError::DuplicateApiTokenHash)
        }
        Err(err) if is_sea_orm_foreign_key_violation(&err) => Err(RepositoryError::MissingUser),
        Err(err) => Err(anyhow::Error::new(err).context(context).into()),
    }
}

fn user_model(user: &User) -> user_entities::ActiveModel {
    user_entities::ActiveModel {
        id: Set(user.id.clone()),
        tenant_id: Set(user.tenant_id.to_string()),
        email: Set(user.email.clone()),
        display_name: Set(user.display_name.clone()),
        role: Set(user.role.as_str().to_owned()),
        created_at: Set(user.created_at.clone()),
    }
}

fn api_token_model(token: &ApiToken, token_hash: &str) -> api_tokens::ActiveModel {
    api_tokens::ActiveModel {
        id: Set(token.id.clone()),
        tenant_id: Set(token.tenant_id.to_string()),
        user_id: Set(token.user_id.clone()),
        name: Set(token.name.clone()),
        token_hash: Set(token_hash.to_owned()),
        created_at: Set(token.created_at.clone()),
        last_used_at: Set(token.last_used_at.clone()),
        revoked_at: Set(token.revoked_at.clone()),
    }
}

pub(super) fn authenticated_from_models(
    token: api_tokens::Model,
    user: user_entities::Model,
) -> RepositoryResult<AuthenticatedUser> {
    Ok(AuthenticatedUser {
        token_id: token.id,
        user: user_from_model(user)?,
    })
}
