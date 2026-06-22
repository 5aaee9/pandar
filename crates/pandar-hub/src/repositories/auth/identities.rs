use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use sqlx::Row;

use crate::{
    db::Database,
    repositories::{
        AuthRepository, AuthenticatedUser, RepositoryError, RepositoryResult,
        auth::authenticated_from_parts, is_foreign_key_violation, is_unique_violation,
    },
};

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

        let result = match &self.database {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "INSERT INTO user_identities (id, tenant_id, user_id, provider, subject, created_at)
                     SELECT ?1, ?2, ?3, ?4, ?5, ?6
                     WHERE EXISTS (SELECT 1 FROM users WHERE id = ?3 AND tenant_id = ?2)",
                )
                .bind(&identity.id)
                .bind(identity.tenant_id.to_string())
                .bind(&identity.user_id)
                .bind(&identity.provider)
                .bind(&identity.subject)
                .bind(&identity.created_at)
                .execute(pool)
                .await
                .map(|result| result.rows_affected())
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO user_identities (id, tenant_id, user_id, provider, subject, created_at)
                     SELECT $1, $2, $3, $4, $5, $6
                     WHERE EXISTS (SELECT 1 FROM users WHERE id = $3 AND tenant_id = $2)",
                )
                .bind(&identity.id)
                .bind(identity.tenant_id.to_string())
                .bind(&identity.user_id)
                .bind(&identity.provider)
                .bind(&identity.subject)
                .bind(&identity.created_at)
                .execute(pool)
                .await
                .map(|result| result.rows_affected())
            }
        };

        match result {
            Ok(0) => Err(RepositoryError::MissingUser),
            Ok(_) => Ok(identity),
            Err(err)
                if is_unique_violation(
                    &err,
                    USER_IDENTITIES_EXTERNAL_UNIQUE_SQLITE,
                    USER_IDENTITIES_EXTERNAL_UNIQUE_POSTGRES,
                ) || is_unique_violation(
                    &err,
                    USER_IDENTITIES_USER_PROVIDER_UNIQUE_SQLITE,
                    USER_IDENTITIES_USER_PROVIDER_UNIQUE_POSTGRES,
                ) =>
            {
                if self.external_identity_exists(&identity).await? {
                    Err(RepositoryError::DuplicateExternalIdentity)
                } else {
                    Err(RepositoryError::DuplicateUserExternalIdentity)
                }
            }
            Err(err) if is_foreign_key_violation(&err) => Err(RepositoryError::MissingUser),
            Err(err) => Err(anyhow::Error::new(err)
                .context("failed to insert external identity")
                .into()),
        }
    }

    pub async fn authenticate_external_identity(
        &self,
        tenant_id: TenantId,
        provider: &str,
        subject: &str,
    ) -> RepositoryResult<Option<AuthenticatedUser>> {
        match &self.database {
            Database::Sqlite(pool) => {
                let row = sqlx::query(
                    "SELECT user_identities.id AS token_id, users.id AS user_id, users.tenant_id, users.email,
                            users.display_name, users.role, users.created_at
                     FROM user_identities
                     JOIN users ON users.id = user_identities.user_id AND users.tenant_id = user_identities.tenant_id
                     WHERE user_identities.tenant_id = ?1
                       AND user_identities.provider = ?2
                       AND user_identities.subject = ?3",
                )
                .bind(tenant_id.to_string())
                .bind(provider)
                .bind(subject)
                .fetch_optional(pool)
                .await
                .context("failed to authenticate SQLite external identity")?;
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
                    "SELECT user_identities.id AS token_id, users.id AS user_id, users.tenant_id, users.email,
                            users.display_name, users.role, users.created_at
                     FROM user_identities
                     JOIN users ON users.id = user_identities.user_id AND users.tenant_id = user_identities.tenant_id
                     WHERE user_identities.tenant_id = $1
                       AND user_identities.provider = $2
                       AND user_identities.subject = $3",
                )
                .bind(tenant_id.to_string())
                .bind(provider)
                .bind(subject)
                .fetch_optional(pool)
                .await
                .context("failed to authenticate PostgreSQL external identity")?;
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

    async fn external_identity_exists(&self, identity: &UserIdentity) -> RepositoryResult<bool> {
        match &self.database {
            Database::Sqlite(pool) => sqlx::query(
                "SELECT 1 FROM user_identities
                 WHERE tenant_id = ?1 AND provider = ?2 AND subject = ?3",
            )
            .bind(identity.tenant_id.to_string())
            .bind(&identity.provider)
            .bind(&identity.subject)
            .fetch_optional(pool)
            .await
            .context("failed to inspect duplicate SQLite external identity")
            .map(|row| row.is_some())
            .map_err(Into::into),
            Database::Postgres(pool) => sqlx::query(
                "SELECT 1 FROM user_identities
                 WHERE tenant_id = $1 AND provider = $2 AND subject = $3",
            )
            .bind(identity.tenant_id.to_string())
            .bind(&identity.provider)
            .bind(&identity.subject)
            .fetch_optional(pool)
            .await
            .context("failed to inspect duplicate PostgreSQL external identity")
            .map(|row| row.is_some())
            .map_err(Into::into),
        }
    }
}

const USER_IDENTITIES_EXTERNAL_UNIQUE_SQLITE: &str =
    "user_identities.tenant_id, user_identities.provider, user_identities.subject";
const USER_IDENTITIES_EXTERNAL_UNIQUE_POSTGRES: &str =
    "user_identities_tenant_id_provider_subject_key";
const USER_IDENTITIES_USER_PROVIDER_UNIQUE_SQLITE: &str =
    "user_identities.tenant_id, user_identities.user_id, user_identities.provider";
const USER_IDENTITIES_USER_PROVIDER_UNIQUE_POSTGRES: &str =
    "user_identities_tenant_id_user_id_provider_key";
