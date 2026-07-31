use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, TransactionTrait,
};
mod helpers;
mod mobile;
mod no_auth_session;
mod revocation;
mod types;

pub use types::{
    AuthenticatedTenantToken, TenantToken, TenantTokenScope, TenantTokenWithPlaintext,
};

#[cfg(test)]
pub(crate) use no_auth_session::test_pause as no_auth_session_test_pause;
pub use no_auth_session::{NoAuthPluginSession, NoAuthPluginSessionOutcome};

use crate::{
    db::{UniqueConstraint, is_foreign_key_violation, is_unique_violation},
    entities::{tenant_tokens, users},
    repositories::{
        AuditActor, AuthRepository, RepositoryError, RepositoryResult,
        audit::insert_audit_event_tx,
        auth::{hash_token, secrets::generate_secret, user_exists},
    },
};
use helpers::{is_expired, tenant_token_audit_event, tenant_token_model};

const TENANT_TOKEN_PREFIX: &str = "pandar_tenant_";
const PLUGIN_TOKEN_PREFIX: &str = "pandar_plugin_";
const MOBILE_TOKEN_PREFIX: &str = "pandar_mobile_";

impl AuthRepository {
    pub async fn create_tenant_token_with_audit(
        &self,
        tenant_id: TenantId,
        name: impl Into<String>,
        scopes: Vec<TenantTokenScope>,
        expires_at: Option<String>,
        actor: AuditActor,
    ) -> RepositoryResult<TenantTokenWithPlaintext> {
        let plaintext_token = generate_secret(TENANT_TOKEN_PREFIX);
        let token = TenantToken {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id,
            name: name.into(),
            scopes,
            created_by_user_id: actor.user_id.clone(),
            created_at: created_at_now(),
            last_used_at: None,
            expires_at,
            revoked_at: None,
        };
        let token_hash = hash_token(&plaintext_token);

        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin tenant token create transaction")?;
        insert_tenant_token(&tx, &token, &token_hash, "failed to insert tenant token").await?;
        insert_audit_event_tx(
            &tx,
            &tenant_token_audit_event(&token, "tenant_token.create", actor),
        )
        .await?;
        tx.commit()
            .await
            .context("failed to commit tenant token create transaction")?;

        Ok(TenantTokenWithPlaintext {
            token,
            plaintext_token,
        })
    }

    pub async fn list_tenant_tokens(
        &self,
        tenant_id: TenantId,
    ) -> RepositoryResult<Vec<TenantToken>> {
        tenant_tokens::Entity::find()
            .filter(tenant_tokens::Column::TenantId.eq(tenant_id.to_string()))
            .order_by_asc(tenant_tokens::Column::CreatedAt)
            .order_by_asc(tenant_tokens::Column::Id)
            .all(&self.database.sea_orm_connection())
            .await
            .context("failed to list tenant tokens")?
            .into_iter()
            .map(tenant_token_from_model)
            .collect()
    }

    pub async fn authenticate_tenant_token(
        &self,
        plaintext_token: &str,
    ) -> RepositoryResult<Option<AuthenticatedTenantToken>> {
        let token_hash = hash_token(plaintext_token);
        let connection = self.database.sea_orm_connection();
        let Some(model) = tenant_tokens::Entity::find()
            .filter(tenant_tokens::Column::TokenHash.eq(token_hash))
            .filter(tenant_tokens::Column::RevokedAt.is_null())
            .one(&connection)
            .await
            .context("failed to authenticate tenant token")?
        else {
            return Ok(None);
        };
        let token = tenant_token_from_model(model.clone())?;
        if is_expired(&token)? {
            return Ok(None);
        }

        let mut active: tenant_tokens::ActiveModel = model.into();
        active.last_used_at = Set(Some(created_at_now()));
        active
            .update(&connection)
            .await
            .context("failed to update tenant token last_used_at")?;

        let session_user = if token.scopes.contains(&TenantTokenScope::MobileSession)
            || token.created_by_user_id.is_some()
                && token.scopes.contains(&TenantTokenScope::PluginStudio)
        {
            let Some(user_id) = token.created_by_user_id.as_deref() else {
                return Ok(None);
            };
            let Some(model) = users::Entity::find_by_id(user_id)
                .filter(users::Column::TenantId.eq(token.tenant_id.to_string()))
                .one(&connection)
                .await
                .context("failed to authenticate session token user")?
            else {
                return Ok(None);
            };
            Some(super::user_from_model(model)?)
        } else {
            None
        };

        Ok(Some(AuthenticatedTenantToken {
            token,
            session_user,
        }))
    }

    pub async fn revoke_tenant_token_with_audit(
        &self,
        tenant_id: TenantId,
        token_id: &str,
        actor: AuditActor,
    ) -> RepositoryResult<TenantToken> {
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin tenant token revoke transaction")?;
        let token = revoke_tenant_token(&tx, tenant_id, token_id).await?;
        insert_audit_event_tx(
            &tx,
            &tenant_token_audit_event(&token, "tenant_token.revoke", actor),
        )
        .await?;
        tx.commit()
            .await
            .context("failed to commit tenant token revoke transaction")?;

        Ok(token)
    }

    pub async fn rotate_tenant_token_with_audit(
        &self,
        tenant_id: TenantId,
        token_id: &str,
        expires_at: Option<String>,
        actor: AuditActor,
    ) -> RepositoryResult<TenantTokenWithPlaintext> {
        let plaintext_token = generate_secret(TENANT_TOKEN_PREFIX);
        let token_hash = hash_token(&plaintext_token);
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin tenant token rotate transaction")?;
        let old_token = revoke_tenant_token(&tx, tenant_id, token_id).await?;
        let token = TenantToken {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id,
            name: old_token.name.clone(),
            scopes: old_token.scopes.clone(),
            created_by_user_id: old_token.created_by_user_id.clone(),
            created_at: created_at_now(),
            last_used_at: None,
            expires_at,
            revoked_at: None,
        };
        insert_tenant_token(
            &tx,
            &token,
            &token_hash,
            "failed to insert rotated tenant token",
        )
        .await?;
        insert_audit_event_tx(
            &tx,
            &tenant_token_audit_event(&token, "tenant_token.rotate", actor),
        )
        .await?;
        tx.commit()
            .await
            .context("failed to commit tenant token rotate transaction")?;

        Ok(TenantTokenWithPlaintext {
            token,
            plaintext_token,
        })
    }

    pub async fn create_plugin_token_from_ticket_tx(
        tx: &sea_orm::DatabaseTransaction,
        tenant_id: TenantId,
        name: impl Into<String>,
        created_by_user_id: Option<String>,
        expires_at: String,
    ) -> RepositoryResult<TenantTokenWithPlaintext> {
        let plaintext_token = generate_secret(PLUGIN_TOKEN_PREFIX);
        let token_hash = hash_token(&plaintext_token);
        let token = TenantToken {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id,
            name: name.into(),
            scopes: vec![TenantTokenScope::PluginStudio],
            created_by_user_id,
            created_at: created_at_now(),
            last_used_at: None,
            expires_at: Some(expires_at),
            revoked_at: None,
        };
        insert_tenant_token(
            tx,
            &token,
            &token_hash,
            "failed to insert plugin tenant token",
        )
        .await?;

        Ok(TenantTokenWithPlaintext {
            token,
            plaintext_token,
        })
    }
}

pub(super) async fn insert_tenant_token<C>(
    connection: &C,
    token: &TenantToken,
    token_hash: &str,
    context: &'static str,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    if let Some(user_id) = &token.created_by_user_id {
        user_exists(
            connection,
            token.tenant_id,
            user_id,
            "failed to check tenant token creator",
        )
        .await?
        .then_some(())
        .ok_or(RepositoryError::MissingUser)?;
    }

    let result = tenant_token_model(token, token_hash)
        .insert(connection)
        .await
        .map(|_| ());
    match result {
        Ok(()) => Ok(()),
        Err(err) if is_unique_violation(&err, UniqueConstraint::TenantTokenHash) => {
            Err(RepositoryError::DuplicateTenantTokenHash)
        }
        Err(err) if is_foreign_key_violation(&err) => Err(RepositoryError::MissingTenant),
        Err(err) => Err(anyhow::Error::new(err).context(context).into()),
    }
}

async fn revoke_tenant_token<C>(
    connection: &C,
    tenant_id: TenantId,
    token_id: &str,
) -> RepositoryResult<TenantToken>
where
    C: ConnectionTrait,
{
    let Some(token) = tenant_tokens::Entity::find_by_id(token_id)
        .filter(tenant_tokens::Column::TenantId.eq(tenant_id.to_string()))
        .one(connection)
        .await
        .context("failed to get tenant token before revoke")?
    else {
        return Err(RepositoryError::MissingTenantToken);
    };

    if token.revoked_at.is_some() {
        return tenant_token_from_model(token);
    }

    let mut active: tenant_tokens::ActiveModel = token.into();
    active.revoked_at = Set(Some(created_at_now()));
    active
        .update(connection)
        .await
        .context("failed to revoke tenant token")
        .map_err(Into::into)
        .and_then(tenant_token_from_model)
}

pub(super) fn tenant_token_from_model(
    model: tenant_tokens::Model,
) -> RepositoryResult<TenantToken> {
    let scope_values = serde_json::from_str::<Vec<String>>(&model.scopes_json)
        .with_context(|| format!("failed to parse tenant token scopes for {}", model.id))?;
    let scopes = scope_values
        .into_iter()
        .map(|scope| TenantTokenScope::parse(&scope))
        .collect::<RepositoryResult<Vec<_>>>()?;

    Ok(TenantToken {
        id: model.id,
        tenant_id: TenantId::parse(&model.tenant_id).map_err(anyhow::Error::from)?,
        name: model.name,
        scopes,
        created_by_user_id: model.created_by_user_id,
        created_at: model.created_at,
        last_used_at: model.last_used_at,
        expires_at: model.expires_at,
        revoked_at: model.revoked_at,
    })
}
