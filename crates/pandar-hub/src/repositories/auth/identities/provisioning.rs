use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use serde_json::json;

use crate::{
    db::Database,
    repositories::{
        AuditEvent, AuthRepository, RepositoryError, RepositoryResult, UserIdentity,
        audit::{build_audit_event, insert_audit_event_postgres, insert_audit_event_sqlite},
        auth::identities::{
            USER_IDENTITIES_EXTERNAL_UNIQUE_POSTGRES, USER_IDENTITIES_EXTERNAL_UNIQUE_SQLITE,
            USER_IDENTITIES_USER_PROVIDER_UNIQUE_POSTGRES,
            USER_IDENTITIES_USER_PROVIDER_UNIQUE_SQLITE,
        },
        is_foreign_key_violation, is_unique_violation,
    },
};

impl AuthRepository {
    pub async fn link_external_identity_with_audit(
        &self,
        tenant_id: TenantId,
        user_id: &str,
        provider: impl Into<String>,
        subject: impl Into<String>,
        actor_user_id: String,
    ) -> RepositoryResult<UserIdentity> {
        let identity = UserIdentity {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id,
            user_id: user_id.to_owned(),
            provider: provider.into(),
            subject: subject.into(),
            created_at: created_at_now(),
        };

        match &self.database {
            Database::Sqlite(pool) => {
                let mut tx = pool
                    .begin()
                    .await
                    .context("failed to begin SQLite identity provisioning transaction")?;
                insert_identity_sqlite(&mut *tx, &identity).await?;
                insert_audit_event_sqlite(
                    &mut *tx,
                    &identity_audit_event(&identity, actor_user_id),
                )
                .await?;
                tx.commit()
                    .await
                    .context("failed to commit SQLite identity provisioning transaction")?;
            }
            Database::Postgres(pool) => {
                let mut tx = pool
                    .begin()
                    .await
                    .context("failed to begin PostgreSQL identity provisioning transaction")?;
                insert_identity_postgres(&mut *tx, &identity).await?;
                insert_audit_event_postgres(
                    &mut *tx,
                    &identity_audit_event(&identity, actor_user_id),
                )
                .await?;
                tx.commit()
                    .await
                    .context("failed to commit PostgreSQL identity provisioning transaction")?;
            }
        }

        Ok(identity)
    }
}

async fn insert_identity_sqlite<'e, E>(executor: E, identity: &UserIdentity) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    map_identity_insert(
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
        .execute(executor)
        .await
        .map(|result| result.rows_affected()),
    )
}

async fn insert_identity_postgres<'e, E>(
    executor: E,
    identity: &UserIdentity,
) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    map_identity_insert(
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
        .execute(executor)
        .await
        .map(|result| result.rows_affected()),
    )
}

fn map_identity_insert(result: Result<u64, sqlx::Error>) -> RepositoryResult<()> {
    match result {
        Ok(0) => Err(RepositoryError::MissingUser),
        Ok(_) => Ok(()),
        Err(err)
            if is_unique_violation(
                &err,
                USER_IDENTITIES_EXTERNAL_UNIQUE_SQLITE,
                USER_IDENTITIES_EXTERNAL_UNIQUE_POSTGRES,
            ) =>
        {
            Err(RepositoryError::DuplicateExternalIdentity)
        }
        Err(err)
            if is_unique_violation(
                &err,
                USER_IDENTITIES_USER_PROVIDER_UNIQUE_SQLITE,
                USER_IDENTITIES_USER_PROVIDER_UNIQUE_POSTGRES,
            ) =>
        {
            Err(RepositoryError::DuplicateUserExternalIdentity)
        }
        Err(err) if is_foreign_key_violation(&err) => Err(RepositoryError::MissingUser),
        Err(err) => Err(anyhow::Error::new(err)
            .context("failed to insert provisioned external identity")
            .into()),
    }
}

fn identity_audit_event(identity: &UserIdentity, actor_user_id: String) -> AuditEvent {
    build_audit_event(crate::repositories::RecordAuditEvent {
        tenant_id: identity.tenant_id,
        actor_type: "user".to_owned(),
        user_id: Some(actor_user_id),
        action: "user_identity.link".to_owned(),
        target_type: "user_identity".to_owned(),
        target_id: Some(identity.id.clone()),
        metadata_json: json!({ "provider": identity.provider, "subject": identity.subject })
            .to_string(),
    })
}
