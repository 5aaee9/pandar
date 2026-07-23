use anyhow::Context;
use pandar_core::{Tenant, TenantId, created_at_now};
use sea_orm::{
    ConnectionTrait, EntityTrait, QueryOrder, QuerySelect, SqliteTransactionMode,
    TransactionOptions, TransactionTrait,
};

use super::{
    TENANT_TOKEN_PREFIX, TenantToken, TenantTokenScope, TenantTokenWithPlaintext,
    insert_tenant_token,
};
use crate::{
    db::Database,
    entities::tenants,
    repositories::{
        AuditActor, AuthRepository, RepositoryResult,
        audit::insert_audit_event_tx,
        auth::{
            hash_token, secrets::generate_secret, tenant_tokens::helpers::tenant_token_audit_event,
        },
    },
};

#[cfg(test)]
pub(crate) mod test_pause;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoAuthPluginSession {
    pub tenant: Tenant,
    pub tenant_token: TenantTokenWithPlaintext,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoAuthPluginSessionOutcome {
    Created(Box<NoAuthPluginSession>),
    MissingTenant,
    AmbiguousTenant,
}

impl AuthRepository {
    pub async fn create_no_auth_plugin_session_with_audit(
        &self,
        name: impl Into<String>,
        expires_at: String,
    ) -> RepositoryResult<NoAuthPluginSessionOutcome> {
        let name = name.into();
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin_with_options(TransactionOptions {
                sqlite_transaction_mode: matches!(&self.database, Database::Sqlite(_))
                    .then_some(SqliteTransactionMode::Immediate),
                ..Default::default()
            })
            .await
            .context("failed to begin no-auth plugin session transaction")?;
        if matches!(&self.database, Database::Postgres(_)) {
            tx.execute_unprepared("LOCK TABLE tenants IN SHARE MODE")
                .await
                .context("failed to lock tenants for no-auth plugin session")?;
        }

        let tenants = tenants::Entity::find()
            .order_by_asc(tenants::Column::CreatedAt)
            .order_by_asc(tenants::Column::Id)
            .limit(2)
            .all(&tx)
            .await
            .context("failed to load tenants for no-auth plugin session")?;
        #[cfg(test)]
        test_pause::wait(&name).await;

        let tenant = match tenants.as_slice() {
            [] => {
                tx.commit()
                    .await
                    .context("failed to commit missing no-auth tenant outcome")?;
                return Ok(NoAuthPluginSessionOutcome::MissingTenant);
            }
            [tenant] => tenant_from_model(tenant.clone())?,
            _ => {
                tx.commit()
                    .await
                    .context("failed to commit ambiguous no-auth tenant outcome")?;
                return Ok(NoAuthPluginSessionOutcome::AmbiguousTenant);
            }
        };

        let plaintext_token = generate_secret(TENANT_TOKEN_PREFIX);
        let actor = AuditActor::no_auth();
        let token = TenantToken {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: tenant.id,
            name,
            scopes: vec![TenantTokenScope::PluginStudio],
            created_by_user_id: actor.user_id.clone(),
            created_at: created_at_now(),
            last_used_at: None,
            expires_at: Some(expires_at),
            revoked_at: None,
        };
        insert_tenant_token(
            &tx,
            &token,
            &hash_token(&plaintext_token),
            "failed to insert no-auth plugin session token",
        )
        .await?;
        insert_audit_event_tx(
            &tx,
            &tenant_token_audit_event(&token, "tenant_token.create", actor),
        )
        .await?;
        tx.commit()
            .await
            .context("failed to commit no-auth plugin session transaction")?;

        Ok(NoAuthPluginSessionOutcome::Created(Box::new(
            NoAuthPluginSession {
                tenant,
                tenant_token: TenantTokenWithPlaintext {
                    token,
                    plaintext_token,
                },
            },
        )))
    }
}

fn tenant_from_model(model: tenants::Model) -> RepositoryResult<Tenant> {
    Tenant::from_parts(
        TenantId::parse(&model.id).map_err(anyhow::Error::from)?,
        model.slug,
        model.display_name,
        model.created_at,
    )
    .map_err(anyhow::Error::from)
    .context("failed to rehydrate no-auth plugin session tenant")
    .map_err(Into::into)
}
