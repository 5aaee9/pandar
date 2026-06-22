use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::{
    db::Database,
    repositories::{
        RepositoryError, RepositoryResult, is_foreign_key_violation, is_unique_violation,
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

        let result = match &self.database {
            Database::Sqlite(pool) => sqlx::query(
                "INSERT INTO users (id, tenant_id, email, display_name, role, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(&user.id)
            .bind(user.tenant_id.to_string())
            .bind(&user.email)
            .bind(&user.display_name)
            .bind(user.role.as_str())
            .bind(&user.created_at)
            .execute(pool)
            .await
            .map(|_| ()),
            Database::Postgres(pool) => sqlx::query(
                "INSERT INTO users (id, tenant_id, email, display_name, role, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(&user.id)
            .bind(user.tenant_id.to_string())
            .bind(&user.email)
            .bind(&user.display_name)
            .bind(user.role.as_str())
            .bind(&user.created_at)
            .execute(pool)
            .await
            .map(|_| ()),
        };

        match result {
            Ok(_) => Ok(user),
            Err(err)
                if is_unique_violation(
                    &err,
                    "users.tenant_id, users.email",
                    "users_tenant_id_email_key",
                ) =>
            {
                Err(RepositoryError::DuplicateUserEmail)
            }
            Err(err) if is_foreign_key_violation(&err) => Err(RepositoryError::MissingTenant),
            Err(err) => Err(anyhow::Error::new(err)
                .context("failed to insert user")
                .into()),
        }
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

        let result = match &self.database {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "INSERT INTO api_tokens (id, tenant_id, user_id, name, token_hash, created_at, last_used_at)
                     SELECT ?1, ?2, ?3, ?4, ?5, ?6, NULL
                     WHERE EXISTS (SELECT 1 FROM users WHERE id = ?3 AND tenant_id = ?2)",
                )
                .bind(&token.id)
                .bind(token.tenant_id.to_string())
                .bind(&token.user_id)
                .bind(&token.name)
                .bind(&token_hash)
                .bind(&token.created_at)
                .execute(pool)
                .await
                .map(|result| result.rows_affected())
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO api_tokens (id, tenant_id, user_id, name, token_hash, created_at, last_used_at)
                     SELECT $1, $2, $3, $4, $5, $6, NULL
                     WHERE EXISTS (SELECT 1 FROM users WHERE id = $3 AND tenant_id = $2)",
                )
                .bind(&token.id)
                .bind(token.tenant_id.to_string())
                .bind(&token.user_id)
                .bind(&token.name)
                .bind(&token_hash)
                .bind(&token.created_at)
                .execute(pool)
                .await
                .map(|result| result.rows_affected())
            }
        };

        match result {
            Ok(0) => Err(RepositoryError::MissingUser),
            Ok(_) => Ok(token),
            Err(err)
                if is_unique_violation(
                    &err,
                    "api_tokens.tenant_id, api_tokens.name",
                    "api_tokens_tenant_id_name_key",
                ) =>
            {
                Err(RepositoryError::DuplicateApiTokenName)
            }
            Err(err)
                if is_unique_violation(
                    &err,
                    "api_tokens.token_hash",
                    "api_tokens_token_hash_key",
                ) =>
            {
                Err(RepositoryError::DuplicateApiTokenHash)
            }
            Err(err) if is_foreign_key_violation(&err) => Err(RepositoryError::MissingUser),
            Err(err) => Err(anyhow::Error::new(err)
                .context("failed to insert api token")
                .into()),
        }
    }

    pub async fn authenticate_bearer(
        &self,
        plaintext_token: &str,
    ) -> RepositoryResult<Option<AuthenticatedUser>> {
        let token_hash = hash_token(plaintext_token);
        match &self.database {
            Database::Sqlite(pool) => {
                let row = sqlx::query(
                    "SELECT api_tokens.id AS token_id, users.id AS user_id, users.tenant_id, users.email,
                            users.display_name, users.role, users.created_at
                     FROM api_tokens
                     JOIN users ON users.id = api_tokens.user_id AND users.tenant_id = api_tokens.tenant_id
                     WHERE api_tokens.token_hash = ?1 AND api_tokens.revoked_at IS NULL",
                )
                .bind(&token_hash)
                .fetch_optional(pool)
                .await
                .context("failed to authenticate SQLite bearer token")?;
                let Some(row) = row else {
                    return Ok(None);
                };
                authenticated_from_parts(
                    row.get("token_id"),
                    row.get("user_id"),
                    row.get("tenant_id"),
                    row.get("email"),
                    row.get("display_name"),
                    row.get("role"),
                    row.get("created_at"),
                )
                .map(Some)
            }
            Database::Postgres(pool) => {
                let row = sqlx::query(
                    "SELECT api_tokens.id AS token_id, users.id AS user_id, users.tenant_id, users.email,
                            users.display_name, users.role, users.created_at
                     FROM api_tokens
                     JOIN users ON users.id = api_tokens.user_id AND users.tenant_id = api_tokens.tenant_id
                     WHERE api_tokens.token_hash = $1 AND api_tokens.revoked_at IS NULL",
                )
                .bind(&token_hash)
                .fetch_optional(pool)
                .await
                .context("failed to authenticate PostgreSQL bearer token")?;
                let Some(row) = row else {
                    return Ok(None);
                };
                authenticated_from_parts(
                    row.get("token_id"),
                    row.get("user_id"),
                    row.get("tenant_id"),
                    row.get("email"),
                    row.get("display_name"),
                    row.get("role"),
                    row.get("created_at"),
                )
                .map(Some)
            }
        }
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

pub(super) fn authenticated_from_parts(
    token_id: String,
    user_id: String,
    tenant_id: String,
    email: String,
    display_name: String,
    role: String,
    created_at: String,
) -> RepositoryResult<AuthenticatedUser> {
    Ok(AuthenticatedUser {
        token_id,
        user: user_from_row(user_id, tenant_id, email, display_name, role, created_at)?,
    })
}
