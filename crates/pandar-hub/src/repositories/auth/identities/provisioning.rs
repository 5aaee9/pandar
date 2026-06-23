use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use sea_orm::TransactionTrait;
use serde_json::json;

use crate::repositories::{
    AuditActor, AuditEvent, AuthRepository, RepositoryResult, UserIdentity,
    audit::{insert_audit_event_tx, record_audit_event},
    auth::identities::insert_identity,
};

impl AuthRepository {
    pub async fn link_external_identity_with_audit(
        &self,
        tenant_id: TenantId,
        user_id: &str,
        provider: impl Into<String>,
        subject: impl Into<String>,
        actor: AuditActor,
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
        insert_audit_event_tx(&tx, &identity_audit_event(&identity, actor)).await?;
        tx.commit()
            .await
            .context("failed to commit identity provisioning transaction")?;

        Ok(identity)
    }
}

fn identity_audit_event(identity: &UserIdentity, actor: AuditActor) -> AuditEvent {
    record_audit_event(
        identity.tenant_id,
        actor,
        "user_identity.link",
        "user_identity",
        Some(identity.id.clone()),
        json!({ "provider": identity.provider }),
    )
}
