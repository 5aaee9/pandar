use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use sea_orm::TransactionTrait;
use serde_json::json;

use crate::repositories::{
    AuditActor, AuditEvent, AuthRepository, RepositoryResult, User, UserRole,
    audit::{insert_audit_event_tx, record_audit_event},
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
        actor: AuditActor,
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
        insert_audit_event_tx(&tx, &user_audit_event(&user, actor)).await?;
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
        actor: AuditActor,
    ) -> RepositoryResult<User> {
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin user role transaction")?;
        let previous_role = select_user_role(&tx, tenant_id, user_id).await?;
        let user = update_user_role(&tx, tenant_id, user_id, role).await?;
        insert_audit_event_tx(&tx, &user_role_audit_event(&user, previous_role, actor)).await?;
        tx.commit()
            .await
            .context("failed to commit user role transaction")?;

        Ok(user)
    }
}

fn user_audit_event(user: &User, actor: AuditActor) -> AuditEvent {
    record_audit_event(
        user.tenant_id,
        actor,
        "user.create",
        "user",
        Some(user.id.clone()),
        json!({ "email": user.email, "role": user.role.as_str() }),
    )
}

fn user_role_audit_event(user: &User, previous_role: UserRole, actor: AuditActor) -> AuditEvent {
    record_audit_event(
        user.tenant_id,
        actor,
        "user.role_update",
        "user",
        Some(user.id.clone()),
        json!({
            "previous_role": previous_role.as_str(),
            "new_role": user.role.as_str()
        }),
    )
}
