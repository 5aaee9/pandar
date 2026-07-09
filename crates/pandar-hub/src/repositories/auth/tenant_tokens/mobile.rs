use pandar_core::{TenantId, created_at_now};

use super::{
    MOBILE_TOKEN_PREFIX, TenantToken, TenantTokenScope, TenantTokenWithPlaintext,
    insert_tenant_token,
};
use crate::repositories::{
    AuthRepository, RepositoryResult,
    auth::{hash_token, secrets::generate_secret},
};

impl AuthRepository {
    pub async fn create_mobile_token_from_ticket_tx(
        tx: &sea_orm::DatabaseTransaction,
        tenant_id: TenantId,
        name: impl Into<String>,
        created_by_user_id: Option<String>,
        expires_at: String,
    ) -> RepositoryResult<TenantTokenWithPlaintext> {
        let plaintext_token = generate_secret(MOBILE_TOKEN_PREFIX);
        let token_hash = hash_token(&plaintext_token);
        let token = TenantToken {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id,
            name: name.into(),
            scopes: vec![TenantTokenScope::All],
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
            "failed to insert mobile tenant token",
        )
        .await?;

        Ok(TenantTokenWithPlaintext {
            token,
            plaintext_token,
        })
    }
}
