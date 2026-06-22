use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use serde_json::json;
use sqlx::Row;

use crate::{
    db::Database,
    repositories::{
        AuditEvent, AuthRepository, RepositoryError, RepositoryResult, User, UserRole,
        audit::{build_audit_event, insert_audit_event_postgres, insert_audit_event_sqlite},
        is_foreign_key_violation, is_unique_violation,
    },
};

use super::super::user_from_row;

impl AuthRepository {
    pub async fn create_user_with_audit(
        &self,
        tenant_id: TenantId,
        email: impl Into<String>,
        display_name: impl Into<String>,
        role: UserRole,
        actor_user_id: String,
    ) -> RepositoryResult<User> {
        let user = User {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id,
            email: email.into(),
            display_name: display_name.into(),
            role,
            created_at: created_at_now(),
        };

        match &self.database {
            Database::Sqlite(pool) => {
                let mut tx = pool
                    .begin()
                    .await
                    .context("failed to begin SQLite user provisioning transaction")?;
                insert_user_sqlite(&mut *tx, &user).await?;
                insert_audit_event_sqlite(&mut *tx, &user_audit_event(&user, actor_user_id))
                    .await?;
                tx.commit()
                    .await
                    .context("failed to commit SQLite user provisioning transaction")?;
            }
            Database::Postgres(pool) => {
                let mut tx = pool
                    .begin()
                    .await
                    .context("failed to begin PostgreSQL user provisioning transaction")?;
                insert_user_postgres(&mut *tx, &user).await?;
                insert_audit_event_postgres(&mut *tx, &user_audit_event(&user, actor_user_id))
                    .await?;
                tx.commit()
                    .await
                    .context("failed to commit PostgreSQL user provisioning transaction")?;
            }
        }

        Ok(user)
    }

    pub async fn update_user_role_with_audit(
        &self,
        tenant_id: TenantId,
        user_id: &str,
        role: UserRole,
        actor_user_id: String,
    ) -> RepositoryResult<User> {
        match &self.database {
            Database::Sqlite(pool) => {
                let mut tx = pool
                    .begin()
                    .await
                    .context("failed to begin SQLite user role transaction")?;
                let previous_role = select_user_role_sqlite(&mut *tx, tenant_id, user_id).await?;
                let user = update_user_role_sqlite(&mut *tx, tenant_id, user_id, role).await?;
                insert_audit_event_sqlite(
                    &mut *tx,
                    &user_role_audit_event(&user, previous_role, actor_user_id),
                )
                .await?;
                tx.commit()
                    .await
                    .context("failed to commit SQLite user role transaction")?;
                Ok(user)
            }
            Database::Postgres(pool) => {
                let mut tx = pool
                    .begin()
                    .await
                    .context("failed to begin PostgreSQL user role transaction")?;
                let previous_role = select_user_role_postgres(&mut *tx, tenant_id, user_id).await?;
                let user = update_user_role_postgres(&mut *tx, tenant_id, user_id, role).await?;
                insert_audit_event_postgres(
                    &mut *tx,
                    &user_role_audit_event(&user, previous_role, actor_user_id),
                )
                .await?;
                tx.commit()
                    .await
                    .context("failed to commit PostgreSQL user role transaction")?;
                Ok(user)
            }
        }
    }
}

async fn insert_user_sqlite<'e, E>(executor: E, user: &User) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    map_user_insert(
        sqlx::query(
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
        .map(|_| ()),
    )
}

async fn insert_user_postgres<'e, E>(executor: E, user: &User) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    map_user_insert(
        sqlx::query(
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
        .map(|_| ()),
    )
}

async fn select_user_role_sqlite<'e, E>(
    executor: E,
    tenant_id: TenantId,
    user_id: &str,
) -> RepositoryResult<UserRole>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query_scalar::<_, String>("SELECT role FROM users WHERE tenant_id = ?1 AND id = ?2")
        .bind(tenant_id.to_string())
        .bind(user_id)
        .fetch_optional(executor)
        .await
        .context("failed to select SQLite user role")?
        .map(|role| UserRole::parse(&role))
        .transpose()?
        .ok_or(RepositoryError::MissingUser)
}

async fn select_user_role_postgres<'e, E>(
    executor: E,
    tenant_id: TenantId,
    user_id: &str,
) -> RepositoryResult<UserRole>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_scalar::<_, String>("SELECT role FROM users WHERE tenant_id = $1 AND id = $2")
        .bind(tenant_id.to_string())
        .bind(user_id)
        .fetch_optional(executor)
        .await
        .context("failed to select PostgreSQL user role")?
        .map(|role| UserRole::parse(&role))
        .transpose()?
        .ok_or(RepositoryError::MissingUser)
}

async fn update_user_role_sqlite<'e, E>(
    executor: E,
    tenant_id: TenantId,
    user_id: &str,
    role: UserRole,
) -> RepositoryResult<User>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let row = sqlx::query(
        "UPDATE users
         SET role = ?1
         WHERE tenant_id = ?2 AND id = ?3
         RETURNING id, tenant_id, email, display_name, role, created_at",
    )
    .bind(role.as_str())
    .bind(tenant_id.to_string())
    .bind(user_id)
    .fetch_one(executor)
    .await
    .context("failed to update SQLite user role")?;
    user_from_row(
        row.get("id"),
        row.get("tenant_id"),
        row.get("email"),
        row.get("display_name"),
        row.get("role"),
        row.get("created_at"),
    )
}

async fn update_user_role_postgres<'e, E>(
    executor: E,
    tenant_id: TenantId,
    user_id: &str,
    role: UserRole,
) -> RepositoryResult<User>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row = sqlx::query(
        "UPDATE users
         SET role = $1
         WHERE tenant_id = $2 AND id = $3
         RETURNING id, tenant_id, email, display_name, role, created_at",
    )
    .bind(role.as_str())
    .bind(tenant_id.to_string())
    .bind(user_id)
    .fetch_one(executor)
    .await
    .context("failed to update PostgreSQL user role")?;
    user_from_row(
        row.get("id"),
        row.get("tenant_id"),
        row.get("email"),
        row.get("display_name"),
        row.get("role"),
        row.get("created_at"),
    )
}

fn map_user_insert(result: Result<(), sqlx::Error>) -> RepositoryResult<()> {
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
            .context("failed to insert provisioned user")
            .into()),
    }
}

fn user_audit_event(user: &User, actor_user_id: String) -> AuditEvent {
    build_audit_event(crate::repositories::RecordAuditEvent {
        tenant_id: user.tenant_id,
        actor_type: "user".to_owned(),
        user_id: Some(actor_user_id),
        action: "user.create".to_owned(),
        target_type: "user".to_owned(),
        target_id: Some(user.id.clone()),
        metadata_json: json!({ "email": user.email, "role": user.role.as_str() }).to_string(),
    })
}

fn user_role_audit_event(
    user: &User,
    previous_role: UserRole,
    actor_user_id: String,
) -> AuditEvent {
    build_audit_event(crate::repositories::RecordAuditEvent {
        tenant_id: user.tenant_id,
        actor_type: "user".to_owned(),
        user_id: Some(actor_user_id),
        action: "user.role_update".to_owned(),
        target_type: "user".to_owned(),
        target_id: Some(user.id.clone()),
        metadata_json: json!({
            "previous_role": previous_role.as_str(),
            "new_role": user.role.as_str()
        })
        .to_string(),
    })
}
