use anyhow::Context;
use pandar_core::{Tenant, created_at_now};
use serde_json::json;

use crate::{
    db::Database,
    repositories::{
        ApiToken, AuditEvent, AuthRepository, RepositoryError, RepositoryResult, User, UserRole,
        audit::{build_audit_event, insert_audit_event_postgres, insert_audit_event_sqlite},
        is_foreign_key_violation, is_unique_violation,
    },
};

use super::hash_token;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrappedTenantAdmin {
    pub tenant: Tenant,
    pub user: User,
    pub api_token: ApiToken,
}

impl AuthRepository {
    pub async fn bootstrap_tenant_admin_with_plaintext_token(
        &self,
        tenant_slug: impl Into<String>,
        tenant_display_name: impl Into<String>,
        admin_email: impl Into<String>,
        admin_display_name: impl Into<String>,
        api_token_name: impl Into<String>,
        plaintext_token: &str,
    ) -> RepositoryResult<BootstrappedTenantAdmin> {
        let tenant = Tenant::new(tenant_slug, tenant_display_name).map_err(anyhow::Error::from)?;
        let user = User {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: tenant.id,
            email: admin_email.into(),
            display_name: admin_display_name.into(),
            role: UserRole::TenantAdmin,
            created_at: created_at_now(),
        };
        let api_token = ApiToken {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: tenant.id,
            user_id: user.id.clone(),
            name: api_token_name.into(),
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
                    .context("failed to begin SQLite bootstrap transaction")?;
                insert_tenant_sqlite(&mut *tx, &tenant).await?;
                insert_user_sqlite(&mut *tx, &user).await?;
                insert_api_token_sqlite(&mut *tx, &api_token, &token_hash).await?;
                for event in bootstrap_audit_events(&tenant, &user, &api_token) {
                    insert_audit_event_sqlite(&mut *tx, &event).await?;
                }
                tx.commit()
                    .await
                    .context("failed to commit SQLite bootstrap transaction")?;
            }
            Database::Postgres(pool) => {
                let mut tx = pool
                    .begin()
                    .await
                    .context("failed to begin PostgreSQL bootstrap transaction")?;
                insert_tenant_postgres(&mut *tx, &tenant).await?;
                insert_user_postgres(&mut *tx, &user).await?;
                insert_api_token_postgres(&mut *tx, &api_token, &token_hash).await?;
                for event in bootstrap_audit_events(&tenant, &user, &api_token) {
                    insert_audit_event_postgres(&mut *tx, &event).await?;
                }
                tx.commit()
                    .await
                    .context("failed to commit PostgreSQL bootstrap transaction")?;
            }
        }

        Ok(BootstrappedTenantAdmin {
            tenant,
            user,
            api_token,
        })
    }
}

async fn insert_tenant_sqlite<'e, E>(executor: E, tenant: &Tenant) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let result = sqlx::query(
        "INSERT INTO tenants (id, slug, display_name, created_at) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(tenant.id.to_string())
    .bind(&tenant.slug)
    .bind(&tenant.display_name)
    .bind(&tenant.created_at)
    .execute(executor)
    .await
    .map(|_| ());
    map_tenant_insert_result(result)
}

async fn insert_tenant_postgres<'e, E>(executor: E, tenant: &Tenant) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let result = sqlx::query(
        "INSERT INTO tenants (id, slug, display_name, created_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(tenant.id.to_string())
    .bind(&tenant.slug)
    .bind(&tenant.display_name)
    .bind(&tenant.created_at)
    .execute(executor)
    .await
    .map(|_| ());
    map_tenant_insert_result(result)
}

async fn insert_user_sqlite<'e, E>(executor: E, user: &User) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let result = sqlx::query(
        "INSERT INTO users (id, tenant_id, email, display_name, role, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&user.id)
    .bind(user.tenant_id.to_string())
    .bind(&user.email)
    .bind(&user.display_name)
    .bind(user.role.as_str())
    .bind(&user.created_at)
    .execute(executor)
    .await
    .map(|_| ());
    map_user_insert_result(result)
}

async fn insert_user_postgres<'e, E>(executor: E, user: &User) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let result = sqlx::query(
        "INSERT INTO users (id, tenant_id, email, display_name, role, created_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(&user.id)
    .bind(user.tenant_id.to_string())
    .bind(&user.email)
    .bind(&user.display_name)
    .bind(user.role.as_str())
    .bind(&user.created_at)
    .execute(executor)
    .await
    .map(|_| ());
    map_user_insert_result(result)
}

async fn insert_api_token_sqlite<'e, E>(
    executor: E,
    token: &ApiToken,
    token_hash: &str,
) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let result = sqlx::query(
        "INSERT INTO api_tokens (id, tenant_id, user_id, name, token_hash, created_at, last_used_at, revoked_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL)",
    )
    .bind(&token.id)
    .bind(token.tenant_id.to_string())
    .bind(&token.user_id)
    .bind(&token.name)
    .bind(token_hash)
    .bind(&token.created_at)
    .execute(executor)
    .await
    .map(|_| ());
    map_api_token_insert_result(result)
}

async fn insert_api_token_postgres<'e, E>(
    executor: E,
    token: &ApiToken,
    token_hash: &str,
) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let result = sqlx::query(
        "INSERT INTO api_tokens (id, tenant_id, user_id, name, token_hash, created_at, last_used_at, revoked_at)
         VALUES ($1, $2, $3, $4, $5, $6, NULL, NULL)",
    )
    .bind(&token.id)
    .bind(token.tenant_id.to_string())
    .bind(&token.user_id)
    .bind(&token.name)
    .bind(token_hash)
    .bind(&token.created_at)
    .execute(executor)
    .await
    .map(|_| ());
    map_api_token_insert_result(result)
}

fn bootstrap_audit_events(tenant: &Tenant, user: &User, token: &ApiToken) -> [AuditEvent; 3] {
    [
        build_audit_event(crate::repositories::RecordAuditEvent {
            tenant_id: tenant.id,
            actor_type: "bootstrap".to_owned(),
            user_id: None,
            action: "tenant.bootstrap".to_owned(),
            target_type: "tenant".to_owned(),
            target_id: Some(tenant.id.to_string()),
            metadata_json: json!({ "tenant_slug": tenant.slug }).to_string(),
        }),
        build_audit_event(crate::repositories::RecordAuditEvent {
            tenant_id: tenant.id,
            actor_type: "bootstrap".to_owned(),
            user_id: None,
            action: "user.create".to_owned(),
            target_type: "user".to_owned(),
            target_id: Some(user.id.clone()),
            metadata_json: json!({ "email": user.email, "role": user.role.as_str() }).to_string(),
        }),
        build_audit_event(crate::repositories::RecordAuditEvent {
            tenant_id: tenant.id,
            actor_type: "bootstrap".to_owned(),
            user_id: None,
            action: "api_token.create".to_owned(),
            target_type: "api_token".to_owned(),
            target_id: Some(token.id.clone()),
            metadata_json: json!({ "name": token.name, "user_id": token.user_id }).to_string(),
        }),
    ]
}

fn map_tenant_insert_result(result: Result<(), sqlx::Error>) -> RepositoryResult<()> {
    match result {
        Ok(()) => Ok(()),
        Err(err)
            if is_unique_violation(&err, "tenants.slug", "tenants_slug_key")
                || is_unique_violation(&err, "tenants.slug", "tenants_slug_key") =>
        {
            Err(RepositoryError::DuplicateTenantSlug)
        }
        Err(err) => Err(anyhow::Error::new(err)
            .context("failed to insert bootstrap tenant")
            .into()),
    }
}

fn map_user_insert_result(result: Result<(), sqlx::Error>) -> RepositoryResult<()> {
    match result {
        Ok(()) => Ok(()),
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
            .context("failed to insert bootstrap user")
            .into()),
    }
}

fn map_api_token_insert_result(result: Result<(), sqlx::Error>) -> RepositoryResult<()> {
    match result {
        Ok(()) => Ok(()),
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
            .context("failed to insert bootstrap api token")
            .into()),
    }
}
