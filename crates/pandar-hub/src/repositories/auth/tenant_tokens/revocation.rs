use anyhow::Context;
use pandar_core::created_at_now;
use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, TransactionTrait};

use super::{TenantToken, TenantTokenScope, tenant_token_from_model};
use crate::{
    entities::tenant_tokens,
    repositories::{
        AuditActor, AuthRepository, RepositoryResult,
        audit::insert_audit_event_tx,
        auth::{hash_token, tenant_tokens::helpers::tenant_token_audit_event},
    },
};

impl AuthRepository {
    pub async fn revoke_plugin_studio_token_with_audit(
        &self,
        plaintext_token: &str,
    ) -> RepositoryResult<Option<TenantToken>> {
        let token_hash = hash_token(plaintext_token);
        let connection = self.database.sea_orm_connection();
        let Some(model) = tenant_tokens::Entity::find()
            .filter(tenant_tokens::Column::TokenHash.eq(token_hash.clone()))
            .one(&connection)
            .await
            .context("failed to load plugin Studio token for self-revoke")?
        else {
            return Ok(None);
        };
        let token = tenant_token_from_model(model)?;
        if token.scopes != [TenantTokenScope::PluginStudio] {
            return Ok(None);
        }

        let tx = connection
            .begin()
            .await
            .context("failed to begin plugin Studio token self-revoke transaction")?;
        let revoked_at = created_at_now();
        let result = tenant_tokens::Entity::update_many()
            .set(tenant_tokens::ActiveModel {
                revoked_at: Set(Some(revoked_at.clone())),
                ..Default::default()
            })
            .filter(tenant_tokens::Column::Id.eq(token.id.clone()))
            .filter(tenant_tokens::Column::TenantId.eq(token.tenant_id.to_string()))
            .filter(tenant_tokens::Column::TokenHash.eq(token_hash.clone()))
            .filter(tenant_tokens::Column::RevokedAt.is_null())
            .exec(&tx)
            .await
            .context("failed to self-revoke plugin Studio token")?;

        if result.rows_affected == 1 {
            let mut revoked = token;
            revoked.revoked_at = Some(revoked_at);
            let actor = AuditActor::plugin_token(
                revoked.created_by_user_id.clone(),
                revoked.id.clone(),
                vec![TenantTokenScope::PluginStudio.as_str()],
            );
            insert_audit_event_tx(
                &tx,
                &tenant_token_audit_event(&revoked, "tenant_token.revoke", actor),
            )
            .await?;
            tx.commit()
                .await
                .context("failed to commit plugin Studio token self-revoke transaction")?;
            return Ok(Some(revoked));
        }

        let revoked = tenant_tokens::Entity::find_by_id(&token.id)
            .filter(tenant_tokens::Column::TenantId.eq(token.tenant_id.to_string()))
            .filter(tenant_tokens::Column::TokenHash.eq(token_hash))
            .one(&tx)
            .await
            .context("failed to reload plugin Studio token after self-revoke race")?
            .map(tenant_token_from_model)
            .transpose()?
            .filter(|token| {
                token.scopes == [TenantTokenScope::PluginStudio] && token.revoked_at.is_some()
            });
        tx.commit()
            .await
            .context("failed to commit repeated plugin Studio token self-revoke transaction")?;
        Ok(revoked)
    }
}
