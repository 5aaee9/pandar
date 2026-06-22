use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use serde_json::json;
use sqlx::Row;

use crate::{
    db::Database,
    repositories::{
        ApiToken, AuditEvent, AuthRepository, RepositoryError, RepositoryResult,
        audit::{build_audit_event, insert_audit_event_postgres, insert_audit_event_sqlite},
        auth::{hash_token, tokens::api_token_from_parts},
        is_foreign_key_violation, is_unique_violation,
    },
};

impl AuthRepository {
    pub async fn create_api_token_with_audit(
        &self,
        tenant_id: TenantId,
        user_id: &str,
        name: impl Into<String>,
        plaintext_token: &str,
        actor_user_id: String,
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

        match &self.database {
            Database::Sqlite(pool) => {
                let mut tx = pool
                    .begin()
                    .await
                    .context("failed to begin SQLite token provisioning transaction")?;
                insert_api_token_sqlite(&mut *tx, &token, &token_hash).await?;
                insert_audit_event_sqlite(&mut *tx, &api_token_audit_event(&token, actor_user_id))
                    .await?;
                tx.commit()
                    .await
                    .context("failed to commit SQLite token provisioning transaction")?;
            }
            Database::Postgres(pool) => {
                let mut tx = pool
                    .begin()
                    .await
                    .context("failed to begin PostgreSQL token provisioning transaction")?;
                insert_api_token_postgres(&mut *tx, &token, &token_hash).await?;
                insert_audit_event_postgres(
                    &mut *tx,
                    &api_token_audit_event(&token, actor_user_id),
                )
                .await?;
                tx.commit()
                    .await
                    .context("failed to commit PostgreSQL token provisioning transaction")?;
            }
        }

        Ok(token)
    }

    pub async fn revoke_api_token_with_audit(
        &self,
        tenant_id: TenantId,
        token_id: &str,
        actor_user_id: String,
    ) -> RepositoryResult<ApiToken> {
        match &self.database {
            Database::Sqlite(pool) => {
                let mut tx = pool
                    .begin()
                    .await
                    .context("failed to begin SQLite token revoke transaction")?;
                let token = revoke_api_token_sqlite(&mut *tx, tenant_id, token_id).await?;
                insert_audit_event_sqlite(
                    &mut *tx,
                    &api_token_revoke_audit_event(&token, actor_user_id),
                )
                .await?;
                tx.commit()
                    .await
                    .context("failed to commit SQLite token revoke transaction")?;
                Ok(token)
            }
            Database::Postgres(pool) => {
                let mut tx = pool
                    .begin()
                    .await
                    .context("failed to begin PostgreSQL token revoke transaction")?;
                let token = revoke_api_token_postgres(&mut *tx, tenant_id, token_id).await?;
                insert_audit_event_postgres(
                    &mut *tx,
                    &api_token_revoke_audit_event(&token, actor_user_id),
                )
                .await?;
                tx.commit()
                    .await
                    .context("failed to commit PostgreSQL token revoke transaction")?;
                Ok(token)
            }
        }
    }
}

async fn insert_api_token_sqlite<'e, E>(
    executor: E,
    token: &ApiToken,
    token_hash: &str,
) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    map_api_token_insert(
        sqlx::query(
            "INSERT INTO api_tokens (id, tenant_id, user_id, name, token_hash, created_at, last_used_at, revoked_at)
             SELECT ?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL
             WHERE EXISTS (SELECT 1 FROM users WHERE id = ?3 AND tenant_id = ?2)",
        )
        .bind(&token.id)
        .bind(token.tenant_id.to_string())
        .bind(&token.user_id)
        .bind(&token.name)
        .bind(token_hash)
        .bind(&token.created_at)
        .execute(executor)
        .await
        .map(|result| result.rows_affected()),
    )
}

async fn insert_api_token_postgres<'e, E>(
    executor: E,
    token: &ApiToken,
    token_hash: &str,
) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    map_api_token_insert(
        sqlx::query(
            "INSERT INTO api_tokens (id, tenant_id, user_id, name, token_hash, created_at, last_used_at, revoked_at)
             SELECT $1, $2, $3, $4, $5, $6, NULL, NULL
             WHERE EXISTS (SELECT 1 FROM users WHERE id = $3 AND tenant_id = $2)",
        )
        .bind(&token.id)
        .bind(token.tenant_id.to_string())
        .bind(&token.user_id)
        .bind(&token.name)
        .bind(token_hash)
        .bind(&token.created_at)
        .execute(executor)
        .await
        .map(|result| result.rows_affected()),
    )
}

async fn revoke_api_token_sqlite<'e, E>(
    executor: E,
    tenant_id: TenantId,
    token_id: &str,
) -> RepositoryResult<ApiToken>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let revoked_at = created_at_now();
    let row = sqlx::query(
        "UPDATE api_tokens
         SET revoked_at = COALESCE(revoked_at, ?1)
         WHERE tenant_id = ?2 AND id = ?3
         RETURNING id, tenant_id, user_id, name, created_at, last_used_at, revoked_at",
    )
    .bind(&revoked_at)
    .bind(tenant_id.to_string())
    .bind(token_id)
    .fetch_optional(executor)
    .await
    .context("failed to revoke SQLite api token")?;
    row.map(|row| {
        api_token_from_parts(
            row.get("id"),
            row.get("tenant_id"),
            row.get("user_id"),
            row.get("name"),
            row.get("created_at"),
            row.get("last_used_at"),
            row.get("revoked_at"),
        )
    })
    .transpose()?
    .ok_or(RepositoryError::MissingApiToken)
}

async fn revoke_api_token_postgres<'e, E>(
    executor: E,
    tenant_id: TenantId,
    token_id: &str,
) -> RepositoryResult<ApiToken>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let revoked_at = created_at_now();
    let row = sqlx::query(
        "UPDATE api_tokens
         SET revoked_at = COALESCE(revoked_at, $1)
         WHERE tenant_id = $2 AND id = $3
         RETURNING id, tenant_id, user_id, name, created_at, last_used_at, revoked_at",
    )
    .bind(&revoked_at)
    .bind(tenant_id.to_string())
    .bind(token_id)
    .fetch_optional(executor)
    .await
    .context("failed to revoke PostgreSQL api token")?;
    row.map(|row| {
        api_token_from_parts(
            row.get("id"),
            row.get("tenant_id"),
            row.get("user_id"),
            row.get("name"),
            row.get("created_at"),
            row.get("last_used_at"),
            row.get("revoked_at"),
        )
    })
    .transpose()?
    .ok_or(RepositoryError::MissingApiToken)
}

fn map_api_token_insert(result: Result<u64, sqlx::Error>) -> RepositoryResult<()> {
    match result {
        Ok(0) => Err(RepositoryError::MissingUser),
        Ok(_) => Ok(()),
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
            if is_unique_violation(&err, "api_tokens.token_hash", "api_tokens_token_hash_key") =>
        {
            Err(RepositoryError::DuplicateApiTokenHash)
        }
        Err(err) if is_foreign_key_violation(&err) => Err(RepositoryError::MissingUser),
        Err(err) => Err(anyhow::Error::new(err)
            .context("failed to insert provisioned api token")
            .into()),
    }
}

fn api_token_audit_event(token: &ApiToken, actor_user_id: String) -> AuditEvent {
    build_audit_event(crate::repositories::RecordAuditEvent {
        tenant_id: token.tenant_id,
        actor_type: "user".to_owned(),
        user_id: Some(actor_user_id),
        action: "api_token.create".to_owned(),
        target_type: "api_token".to_owned(),
        target_id: Some(token.id.clone()),
        metadata_json: json!({ "name": token.name, "user_id": token.user_id }).to_string(),
    })
}

fn api_token_revoke_audit_event(token: &ApiToken, actor_user_id: String) -> AuditEvent {
    build_audit_event(crate::repositories::RecordAuditEvent {
        tenant_id: token.tenant_id,
        actor_type: "user".to_owned(),
        user_id: Some(actor_user_id),
        action: "api_token.revoke".to_owned(),
        target_type: "api_token".to_owned(),
        target_id: Some(token.id.clone()),
        metadata_json: json!({ "name": token.name, "user_id": token.user_id }).to_string(),
    })
}
