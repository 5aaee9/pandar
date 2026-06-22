use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use sea_orm::TransactionTrait;
use serde_json::json;

use crate::repositories::{
    ApiToken, AuditEvent, AuthRepository, RepositoryResult,
    audit::{build_audit_event, insert_audit_event_tx},
    auth::{hash_token, insert_api_token, tokens::revoke_api_token},
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

        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin token provisioning transaction")?;
        insert_api_token(
            &tx,
            &token,
            &token_hash,
            "failed to insert provisioned api token",
        )
        .await?;
        insert_audit_event_tx(&tx, &api_token_audit_event(&token, actor_user_id)).await?;
        tx.commit()
            .await
            .context("failed to commit token provisioning transaction")?;

        Ok(token)
    }

    pub async fn revoke_api_token_with_audit(
        &self,
        tenant_id: TenantId,
        token_id: &str,
        actor_user_id: String,
    ) -> RepositoryResult<ApiToken> {
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin token revoke transaction")?;
        let token = revoke_api_token(&tx, tenant_id, token_id).await?;
        insert_audit_event_tx(&tx, &api_token_revoke_audit_event(&token, actor_user_id)).await?;
        tx.commit()
            .await
            .context("failed to commit token revoke transaction")?;

        Ok(token)
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
