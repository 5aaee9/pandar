use anyhow::Context;
use pandar_core::{Tenant, TenantId, created_at_now};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    DatabaseTransaction, DbBackend, EntityTrait, QueryFilter, SqliteTransactionMode, Statement,
    TransactionOptions, TransactionTrait, Value,
};
use serde_json::json;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    entities::{join_links, tenants, user_identities, users},
    repositories::{
        AuditActor, AuditEvent, RepositoryError, RepositoryResult, User, UserIdentity,
        audit::record_audit_event,
        auth::{UserRole, onboarding::JoinLink, user_from_model},
        is_sea_orm_foreign_key_violation, is_sea_orm_unique_violation,
    },
};

pub(super) async fn begin_onboarding_write_transaction(
    connection: &DatabaseConnection,
) -> Result<DatabaseTransaction, sea_orm::DbErr> {
    match connection.get_database_backend() {
        DbBackend::Sqlite => {
            connection
                .begin_with_options(TransactionOptions {
                    sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
                    ..Default::default()
                })
                .await
        }
        _ => connection.begin().await,
    }
}

pub(super) async fn insert_tenant<C>(connection: &C, tenant: &Tenant) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    let model = tenants::ActiveModel {
        id: Set(tenant.id.to_string()),
        slug: Set(tenant.slug.clone()),
        display_name: Set(tenant.display_name.clone()),
        created_at: Set(tenant.created_at.clone()),
    };
    let result = model.insert(connection).await.map(|_| ());
    match result {
        Ok(()) => Ok(()),
        Err(err) if is_sea_orm_unique_violation(&err, "tenants.slug", "tenants_slug_key") => {
            Err(RepositoryError::DuplicateTenantSlug)
        }
        Err(err) => Err(anyhow::Error::new(err)
            .context("failed to insert self-created tenant")
            .into()),
    }
}

pub(super) async fn insert_join_link<C>(
    connection: &C,
    join_link: &JoinLink,
    token_hash: &str,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    let result = join_link_model(join_link, token_hash)
        .insert(connection)
        .await
        .map(|_| ());
    match result {
        Ok(()) => Ok(()),
        Err(err)
            if is_sea_orm_unique_violation(
                &err,
                "join_links.token_hash",
                "join_links_token_hash_key",
            ) =>
        {
            Err(RepositoryError::DuplicateJoinLinkHash)
        }
        Err(err) if is_sea_orm_foreign_key_violation(&err) => Err(RepositoryError::MissingTenant),
        Err(err) => Err(anyhow::Error::new(err)
            .context("failed to insert join link")
            .into()),
    }
}

pub(super) async fn revoke_join_link<C>(
    connection: &C,
    tenant_id: TenantId,
    join_link_id: &str,
) -> RepositoryResult<JoinLink>
where
    C: ConnectionTrait,
{
    let Some(join_link) = join_links::Entity::find_by_id(join_link_id)
        .filter(join_links::Column::TenantId.eq(tenant_id.to_string()))
        .one(connection)
        .await
        .context("failed to get join link before revoke")?
    else {
        return Err(RepositoryError::InvalidJoinLink);
    };
    if join_link.revoked_at.is_some() {
        return join_link_from_model(join_link);
    }

    let mut active: join_links::ActiveModel = join_link.into();
    active.revoked_at = Set(Some(created_at_now()));
    active
        .update(connection)
        .await
        .context("failed to revoke join link")
        .map_err(Into::into)
        .and_then(join_link_from_model)
}

pub(super) async fn load_valid_join_link_by_hash<C>(
    connection: &C,
    token_hash: &str,
    now: &str,
) -> RepositoryResult<join_links::Model>
where
    C: ConnectionTrait,
{
    let Some(join_link) = join_links::Entity::find()
        .filter(join_links::Column::TokenHash.eq(token_hash))
        .one(connection)
        .await
        .context("failed to load join link")?
    else {
        return Err(RepositoryError::InvalidJoinLink);
    };
    if join_link.revoked_at.is_some()
        || join_link.used_count >= join_link.max_uses
        || join_link.expires_at.as_str() <= now
    {
        return Err(RepositoryError::InvalidJoinLink);
    }
    Ok(join_link)
}

pub(super) async fn consume_join_link_use_tx(
    tx: &DatabaseTransaction,
    join_link_id: &str,
    now: &str,
) -> RepositoryResult<bool> {
    let (sql, values) = match tx.get_database_backend() {
        DbBackend::Postgres => (
            "UPDATE join_links SET used_count = used_count + 1 WHERE id = $1 AND used_count < max_uses AND revoked_at IS NULL AND expires_at > $2",
            vec![Value::from(join_link_id), Value::from(now)],
        ),
        _ => (
            "UPDATE join_links SET used_count = used_count + 1 WHERE id = ? AND used_count < max_uses AND revoked_at IS NULL AND expires_at > ?",
            vec![Value::from(join_link_id), Value::from(now)],
        ),
    };
    let statement = Statement::from_sql_and_values(tx.get_database_backend(), sql, values);
    let result = tx
        .execute_raw(statement)
        .await
        .context("failed to consume join link use")?;
    Ok(result.rows_affected() == 1)
}

pub(super) async fn find_external_user_tx<C>(
    connection: &C,
    tenant_id: TenantId,
    provider: &str,
    subject: &str,
) -> RepositoryResult<Option<User>>
where
    C: ConnectionTrait,
{
    let Some(identity) = user_identities::Entity::find()
        .filter(user_identities::Column::TenantId.eq(tenant_id.to_string()))
        .filter(user_identities::Column::Provider.eq(provider))
        .filter(user_identities::Column::Subject.eq(subject))
        .one(connection)
        .await
        .context("failed to load existing external member identity")?
    else {
        return Ok(None);
    };
    users::Entity::find_by_id(identity.user_id)
        .filter(users::Column::TenantId.eq(tenant_id.to_string()))
        .one(connection)
        .await
        .context("failed to load existing external member user")?
        .map(user_from_model)
        .transpose()
}

pub(super) async fn load_tenant<C>(connection: &C, tenant_id: TenantId) -> RepositoryResult<Tenant>
where
    C: ConnectionTrait,
{
    tenants::Entity::find_by_id(tenant_id.to_string())
        .one(connection)
        .await
        .context("failed to load onboarding tenant")?
        .map(tenant_from_model)
        .transpose()?
        .ok_or(RepositoryError::MissingTenant)
}

pub(super) fn join_link_from_model(model: join_links::Model) -> RepositoryResult<JoinLink> {
    Ok(JoinLink {
        id: model.id,
        tenant_id: model.tenant_id,
        role: UserRole::parse(&model.role)?,
        email_constraint: model.email_constraint,
        expires_at: model.expires_at,
        max_uses: model.max_uses,
        used_count: model.used_count,
        created_by_user_id: model.created_by_user_id,
        revoked_at: model.revoked_at,
        created_at: model.created_at,
    })
}

pub(super) fn join_link_model(join_link: &JoinLink, token_hash: &str) -> join_links::ActiveModel {
    join_links::ActiveModel {
        id: Set(join_link.id.clone()),
        tenant_id: Set(join_link.tenant_id.clone()),
        token_hash: Set(token_hash.to_owned()),
        role: Set(join_link.role.as_str().to_owned()),
        email_constraint: Set(join_link.email_constraint.clone()),
        expires_at: Set(join_link.expires_at.clone()),
        max_uses: Set(join_link.max_uses),
        used_count: Set(join_link.used_count),
        created_by_user_id: Set(join_link.created_by_user_id.clone()),
        revoked_at: Set(join_link.revoked_at.clone()),
        created_at: Set(join_link.created_at.clone()),
    }
}

pub(super) fn tenant_from_model(model: tenants::Model) -> RepositoryResult<Tenant> {
    Tenant::from_parts(
        TenantId::parse(&model.id).map_err(anyhow::Error::from)?,
        model.slug,
        model.display_name,
        model.created_at,
    )
    .map_err(anyhow::Error::from)
    .context("failed to rehydrate onboarding tenant")
    .map_err(RepositoryError::from)
}

pub(super) fn format_timestamp(value: OffsetDateTime) -> RepositoryResult<String> {
    value
        .format(&Rfc3339)
        .context("failed to format onboarding timestamp")
        .map_err(RepositoryError::from)
}

pub(super) fn user_external_projection_audit_event(
    user: &User,
    identity: &UserIdentity,
    actor: AuditActor,
) -> AuditEvent {
    record_audit_event(
        user.tenant_id,
        actor,
        "user.external_projection_create",
        "user",
        Some(user.id.clone()),
        json!({
            "email": user.email,
            "role": user.role.as_str(),
            "provider": identity.provider,
        }),
    )
}

pub(super) fn tenant_self_create_audit_event(tenant: &Tenant, actor: AuditActor) -> AuditEvent {
    record_audit_event(
        tenant.id,
        actor,
        "tenant.self_create",
        "tenant",
        Some(tenant.id.to_string()),
        json!({ "tenant_slug": tenant.slug }),
    )
}

pub(super) fn join_link_audit_event(
    join_link: &JoinLink,
    action: &'static str,
    actor: AuditActor,
) -> AuditEvent {
    record_audit_event(
        TenantId::parse(&join_link.tenant_id).expect("join link tenant id should be valid"),
        actor,
        action,
        "join_link",
        Some(join_link.id.clone()),
        json!({
            "role": join_link.role.as_str(),
            "email_constraint": join_link.email_constraint,
            "max_uses": join_link.max_uses,
        }),
    )
}
