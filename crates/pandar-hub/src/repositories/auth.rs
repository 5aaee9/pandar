use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    db::{Database, UniqueConstraint, is_foreign_key_violation, is_unique_violation},
    entities::users as user_entities,
    repositories::{RepositoryError, RepositoryResult},
};

mod bootstrap;
mod identities;
mod onboarding;
mod plugin_tickets;
pub(crate) mod secrets;
mod tenant_tokens;
mod users;

pub use identities::UserIdentity;
pub use onboarding::{
    AcceptedJoinLink, ExternalIdentityProfile, ExternalMembership, JoinLink, JoinLinkWithPlaintext,
};
pub use plugin_tickets::{
    PluginLoginTicket, PluginLoginTicketExchange, PluginLoginTicketWithPlaintext,
};
#[cfg(test)]
pub(crate) use tenant_tokens::no_auth_session_test_pause;
pub use tenant_tokens::{
    AuthenticatedTenantToken, NoAuthPluginSession, NoAuthPluginSessionOutcome, TenantToken,
    TenantTokenScope, TenantTokenWithPlaintext,
};

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
pub struct AuthenticatedUser {
    pub user: User,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthenticatedPrincipal {
    User(AuthenticatedUser),
    TenantToken(Box<AuthenticatedTenantToken>),
    NoAuth { tenant_id: TenantId },
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
}

pub(crate) fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
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

pub(super) fn authenticated_from_user(user: User) -> RepositoryResult<AuthenticatedUser> {
    Ok(AuthenticatedUser { user })
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
        Err(err) if is_unique_violation(&err, UniqueConstraint::UserEmail) => {
            Err(RepositoryError::DuplicateUserEmail)
        }
        Err(err) if is_foreign_key_violation(&err) => Err(RepositoryError::MissingTenant),
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
