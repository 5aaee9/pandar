use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use sea_orm::TransactionTrait;
use serde_json::json;

use crate::repositories::{
    AuditEvent, AuthRepository, RepositoryResult, UserIdentity,
    audit::{build_audit_event, insert_audit_event_tx},
    auth::identities::insert_identity,
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

        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin identity provisioning transaction")?;
        insert_identity(
            &tx,
            &identity,
            "failed to insert provisioned external identity",
        )
        .await?;
        insert_audit_event_tx(&tx, &identity_audit_event(&identity, actor_user_id)).await?;
        tx.commit()
            .await
            .context("failed to commit identity provisioning transaction")?;

        Ok(identity)
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
