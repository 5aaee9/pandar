use anyhow::Context;
use pandar_core::TenantId;
use sea_orm::TransactionTrait;
use serde::Serialize;

use crate::repositories::{
    AuditActor, AuditEvent, AuthRepository, RepositoryError, RepositoryResult, User, UserRole,
    audit::{audit_metadata, insert_audit_event_tx, record_audit_event},
    auth::users::{
        count_tenant_admins, delete_user, select_user, select_user_role, update_user_role,
    },
};

#[derive(Serialize)]
struct UserRoleAuditMetadata<'a> {
    previous_role: &'a str,
    new_role: &'a str,
}

#[derive(Serialize)]
struct UserRemoveAuditMetadata<'a> {
    email: &'a str,
    role: &'a str,
}

impl AuthRepository {
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

fn user_role_audit_event(user: &User, previous_role: UserRole, actor: AuditActor) -> AuditEvent {
    record_audit_event(
        user.tenant_id,
        actor,
        "user.role_update",
        "user",
        Some(user.id.clone()),
        audit_metadata(UserRoleAuditMetadata {
            previous_role: previous_role.as_str(),
            new_role: user.role.as_str(),
        }),
    )
}

impl AuthRepository {
    pub async fn remove_user_with_audit(
        &self,
        tenant_id: TenantId,
        user_id: &str,
        actor: AuditActor,
    ) -> RepositoryResult<User> {
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin user removal transaction")?;
        let user = select_user(&tx, tenant_id, user_id).await?;
        if user.role == UserRole::TenantAdmin && count_tenant_admins(&tx, tenant_id).await? <= 1 {
            return Err(RepositoryError::LastTenantAdmin);
        }
        delete_user(&tx, tenant_id, user_id).await?;
        insert_audit_event_tx(&tx, &user_remove_audit_event(&user, actor)).await?;
        tx.commit()
            .await
            .context("failed to commit user removal transaction")?;

        Ok(user)
    }
}

fn user_remove_audit_event(user: &User, actor: AuditActor) -> AuditEvent {
    record_audit_event(
        user.tenant_id,
        actor,
        "user.remove",
        "user",
        Some(user.id.clone()),
        audit_metadata(UserRemoveAuditMetadata {
            email: &user.email,
            role: user.role.as_str(),
        }),
    )
}
