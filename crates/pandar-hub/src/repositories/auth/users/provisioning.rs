use anyhow::Context;
use pandar_core::TenantId;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::Serialize;

use crate::{
    db::ConnectionDialectExt,
    entities::users,
    repositories::{
        AuditActor, AuditEvent, AuthRepository, RepositoryError, RepositoryResult, User, UserRole,
        audit::{audit_metadata, insert_audit_event_tx, record_audit_event},
        auth::users::{delete_user, update_user_role},
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
        let tx = self
            .database
            .begin_write_transaction()
            .await
            .context("failed to begin user role transaction")?;
        let previous_role = select_user_locked(&tx, tenant_id, user_id).await?.role;
        if previous_role == UserRole::TenantAdmin && role != UserRole::TenantAdmin {
            ensure_other_admin_remains(&tx, tenant_id).await?;
        }
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
        let tx = self
            .database
            .begin_write_transaction()
            .await
            .context("failed to begin user removal transaction")?;
        let user = select_user_locked(&tx, tenant_id, user_id).await?;
        if user.role == UserRole::TenantAdmin {
            ensure_other_admin_remains(&tx, tenant_id).await?;
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

/// Read one tenant user row under the dialect write lock so concurrent role
/// updates and removals observe committed state instead of racing.
async fn select_user_locked<C>(
    connection: &C,
    tenant_id: TenantId,
    user_id: &str,
) -> RepositoryResult<User>
where
    C: sea_orm::ConnectionTrait + ConnectionDialectExt,
{
    let select = users::Entity::find_by_id(user_id)
        .filter(users::Column::TenantId.eq(tenant_id.to_string()));
    connection
        .lock_for_update(select)
        .one(connection)
        .await
        .context("failed to lock user")?
        .map(super::super::user_from_model)
        .transpose()?
        .ok_or(RepositoryError::MissingUser)
}

/// Lock every tenant-admin row before deciding whether the last admin may be
/// demoted or removed, so concurrent membership changes serialize on the same
/// rows instead of each counting stale state.
async fn ensure_other_admin_remains<C>(connection: &C, tenant_id: TenantId) -> RepositoryResult<()>
where
    C: sea_orm::ConnectionTrait + ConnectionDialectExt,
{
    let select = users::Entity::find()
        .filter(users::Column::TenantId.eq(tenant_id.to_string()))
        .filter(users::Column::Role.eq(UserRole::TenantAdmin.as_str()));
    let admins = connection
        .lock_for_update(select)
        .all(connection)
        .await
        .context("failed to lock tenant admins")?;
    if admins.len() <= 1 {
        return Err(RepositoryError::LastTenantAdmin);
    }
    Ok(())
}
