use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use sea_orm::TransactionTrait;
use serde_json::json;

use crate::repositories::{
    AuditEvent, AuthRepository, RepositoryResult, User, UserRole,
    audit::{build_audit_event, insert_audit_event_tx},
    auth::{
        insert_user,
        users::{select_user_role, update_user_role},
    },
};

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

        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin user provisioning transaction")?;
        insert_user(&tx, &user, "failed to insert provisioned user").await?;
        insert_audit_event_tx(&tx, &user_audit_event(&user, actor_user_id)).await?;
        tx.commit()
            .await
            .context("failed to commit user provisioning transaction")?;

        Ok(user)
    }

    pub async fn update_user_role_with_audit(
        &self,
        tenant_id: TenantId,
        user_id: &str,
        role: UserRole,
        actor_user_id: String,
    ) -> RepositoryResult<User> {
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin user role transaction")?;
        let previous_role = select_user_role(&tx, tenant_id, user_id).await?;
        let user = update_user_role(&tx, tenant_id, user_id, role).await?;
        insert_audit_event_tx(
            &tx,
            &user_role_audit_event(&user, previous_role, actor_user_id),
        )
        .await?;
        tx.commit()
            .await
            .context("failed to commit user role transaction")?;

        Ok(user)
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
