use anyhow::Context;
use pandar_core::{Tenant, TenantId, created_at_now};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    DatabaseTransaction, DbBackend, EntityTrait, QueryFilter, QueryOrder, SqliteTransactionMode,
    Statement, TransactionOptions, TransactionTrait, Value,
};
use serde::Serialize;
use serde_json::json;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    entities::{join_links, tenants, user_identities, users},
    repositories::{
        AuditActor, AuditEvent, AuthRepository, RepositoryError, RepositoryResult, User,
        UserIdentity, UserRole,
        audit::{insert_audit_event_tx, record_audit_event},
        auth::{hash_token, identities::insert_identity, insert_user, secrets::generate_secret},
        is_sea_orm_foreign_key_violation, is_sea_orm_unique_violation,
    },
};

const JOIN_LINK_PREFIX: &str = "pandar_join";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalIdentityProfile {
    pub provider: String,
    pub subject: String,
    pub email: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalMembership {
    pub tenant: Tenant,
    pub user: User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JoinLink {
    pub id: String,
    pub tenant_id: String,
    pub role: UserRole,
    pub email_constraint: Option<String>,
    pub expires_at: String,
    pub max_uses: i32,
    pub used_count: i32,
    pub created_by_user_id: Option<String>,
    pub revoked_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinLinkWithPlaintext {
    pub join_link: JoinLink,
    pub plaintext_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedJoinLink {
    pub tenant: Tenant,
    pub user: User,
    pub created: bool,
}

impl AuthRepository {
    pub async fn list_external_memberships(
        &self,
        provider: &str,
        subject: &str,
    ) -> RepositoryResult<Vec<ExternalMembership>> {
        let connection = self.database.sea_orm_connection();
        let rows = user_identities::Entity::find()
            .filter(user_identities::Column::Provider.eq(provider))
            .filter(user_identities::Column::Subject.eq(subject))
            .order_by_asc(user_identities::Column::CreatedAt)
            .order_by_asc(user_identities::Column::Id)
            .all(&connection)
            .await
            .context("failed to list external memberships")?;
        let mut memberships = Vec::with_capacity(rows.len());
        for identity in rows {
            let Some(user) = users::Entity::find_by_id(identity.user_id.clone())
                .filter(users::Column::TenantId.eq(identity.tenant_id.clone()))
                .one(&connection)
                .await
                .context("failed to load external membership user")?
            else {
                continue;
            };
            let Some(tenant) = tenants::Entity::find_by_id(identity.tenant_id.clone())
                .one(&connection)
                .await
                .context("failed to load external membership tenant")?
            else {
                continue;
            };
            memberships.push(ExternalMembership {
                tenant: tenant_from_model(tenant)?,
                user: super::user_from_model(user)?,
            });
        }
        Ok(memberships)
    }

    pub async fn self_create_tenant_for_external_identity(
        &self,
        slug: impl Into<String>,
        display_name: impl Into<String>,
        profile: ExternalIdentityProfile,
    ) -> RepositoryResult<ExternalMembership> {
        let tenant = Tenant::new(slug, display_name).map_err(anyhow::Error::from)?;
        let user = User {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: tenant.id,
            email: profile.email,
            display_name: profile.display_name,
            role: UserRole::TenantAdmin,
            created_at: created_at_now(),
        };
        let identity = UserIdentity {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: tenant.id,
            user_id: user.id.clone(),
            provider: profile.provider,
            subject: profile.subject,
            created_at: created_at_now(),
        };

        let connection = self.database.sea_orm_connection();
        let tx = begin_onboarding_write_transaction(&connection)
            .await
            .context("failed to begin tenant self-create transaction")?;
        insert_tenant(&tx, &tenant).await?;
        insert_user(&tx, &user, "failed to insert external tenant admin").await?;
        insert_identity(&tx, &identity, "failed to link external tenant admin").await?;
        let actor = AuditActor::user(user.id.clone());
        insert_audit_event_tx(
            &tx,
            &user_external_projection_audit_event(&user, &identity, actor.clone()),
        )
        .await?;
        insert_audit_event_tx(&tx, &tenant_self_create_audit_event(&tenant, actor)).await?;
        tx.commit()
            .await
            .context("failed to commit tenant self-create transaction")?;

        Ok(ExternalMembership { tenant, user })
    }

    pub async fn create_join_link_with_audit(
        &self,
        tenant_id: TenantId,
        role: UserRole,
        email_constraint: Option<String>,
        expires_in_seconds: i64,
        max_uses: i32,
        actor: AuditActor,
    ) -> RepositoryResult<JoinLinkWithPlaintext> {
        let plaintext_token = generate_secret(JOIN_LINK_PREFIX);
        let token_hash = hash_token(&plaintext_token);
        let now = OffsetDateTime::now_utc();
        let join_link = JoinLink {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: tenant_id.to_string(),
            role,
            email_constraint,
            expires_at: format_timestamp(now + Duration::seconds(expires_in_seconds))?,
            max_uses,
            used_count: 0,
            created_by_user_id: actor.user_id.clone(),
            revoked_at: None,
            created_at: format_timestamp(now)?,
        };

        let connection = self.database.sea_orm_connection();
        let tx = begin_onboarding_write_transaction(&connection)
            .await
            .context("failed to begin join link create transaction")?;
        insert_join_link(&tx, &join_link, &token_hash).await?;
        insert_audit_event_tx(
            &tx,
            &join_link_audit_event(&join_link, "join_link.create", actor),
        )
        .await?;
        tx.commit()
            .await
            .context("failed to commit join link create transaction")?;

        Ok(JoinLinkWithPlaintext {
            join_link,
            plaintext_token,
        })
    }

    pub async fn list_join_links_for_tenant(
        &self,
        tenant_id: TenantId,
    ) -> RepositoryResult<Vec<JoinLink>> {
        join_links::Entity::find()
            .filter(join_links::Column::TenantId.eq(tenant_id.to_string()))
            .order_by_asc(join_links::Column::CreatedAt)
            .order_by_asc(join_links::Column::Id)
            .all(&self.database.sea_orm_connection())
            .await
            .context("failed to list join links")?
            .into_iter()
            .map(join_link_from_model)
            .collect()
    }

    pub async fn revoke_join_link_with_audit(
        &self,
        tenant_id: TenantId,
        join_link_id: &str,
        actor: AuditActor,
    ) -> RepositoryResult<JoinLink> {
        let connection = self.database.sea_orm_connection();
        let tx = begin_onboarding_write_transaction(&connection)
            .await
            .context("failed to begin join link revoke transaction")?;
        let join_link = revoke_join_link(&tx, tenant_id, join_link_id).await?;
        insert_audit_event_tx(
            &tx,
            &join_link_audit_event(&join_link, "join_link.revoke", actor),
        )
        .await?;
        tx.commit()
            .await
            .context("failed to commit join link revoke transaction")?;
        Ok(join_link)
    }

    pub async fn accept_join_link(
        &self,
        plaintext_token: &str,
        profile: ExternalIdentityProfile,
    ) -> RepositoryResult<AcceptedJoinLink> {
        let token_hash = hash_token(plaintext_token);
        let now = format_timestamp(OffsetDateTime::now_utc())?;
        let connection = self.database.sea_orm_connection();
        let tx = begin_onboarding_write_transaction(&connection)
            .await
            .context("failed to begin join link accept transaction")?;
        let link_model = load_valid_join_link_by_hash(&tx, &token_hash, &now).await?;
        let join_link = join_link_from_model(link_model.clone())?;
        let tenant_id = TenantId::parse(&join_link.tenant_id).map_err(anyhow::Error::from)?;
        if let Some(email_constraint) = &join_link.email_constraint
            && !email_constraint.eq_ignore_ascii_case(&profile.email)
        {
            return Err(RepositoryError::JoinLinkEmailMismatch);
        }
        if let Some(existing) =
            find_external_user_tx(&tx, tenant_id, &profile.provider, &profile.subject).await?
        {
            let tenant = load_tenant(&tx, tenant_id).await?;
            tx.commit()
                .await
                .context("failed to commit existing join link accept transaction")?;
            return Ok(AcceptedJoinLink {
                tenant,
                user: existing,
                created: false,
            });
        }
        if !consume_join_link_use_tx(&tx, &join_link.id, &now).await? {
            return Err(RepositoryError::InvalidJoinLink);
        }

        let user = User {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id,
            email: profile.email,
            display_name: profile.display_name,
            role: join_link.role,
            created_at: created_at_now(),
        };
        let identity = UserIdentity {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id,
            user_id: user.id.clone(),
            provider: profile.provider,
            subject: profile.subject,
            created_at: created_at_now(),
        };
        insert_user(&tx, &user, "failed to insert join link user").await?;
        insert_identity(&tx, &identity, "failed to insert join link identity").await?;
        insert_audit_event_tx(
            &tx,
            &user_external_projection_audit_event(
                &user,
                &identity,
                AuditActor::user(user.id.clone()),
            ),
        )
        .await?;
        insert_audit_event_tx(
            &tx,
            &record_audit_event(
                tenant_id,
                AuditActor::user(user.id.clone()),
                "join_link.accept",
                "join_link",
                Some(join_link.id),
                json!({ "role": user.role.as_str(), "email": user.email }),
            ),
        )
        .await?;
        let tenant = load_tenant(&tx, tenant_id).await?;
        tx.commit()
            .await
            .context("failed to commit join link accept transaction")?;

        Ok(AcceptedJoinLink {
            tenant,
            user,
            created: true,
        })
    }
}

async fn begin_onboarding_write_transaction(
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

async fn insert_tenant<C>(connection: &C, tenant: &Tenant) -> RepositoryResult<()>
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

async fn insert_join_link<C>(
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

async fn revoke_join_link<C>(
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

async fn load_valid_join_link_by_hash<C>(
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

async fn consume_join_link_use_tx(
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

async fn find_external_user_tx<C>(
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
        .map(super::user_from_model)
        .transpose()
}

async fn load_tenant<C>(connection: &C, tenant_id: TenantId) -> RepositoryResult<Tenant>
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

fn join_link_from_model(model: join_links::Model) -> RepositoryResult<JoinLink> {
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

fn join_link_model(join_link: &JoinLink, token_hash: &str) -> join_links::ActiveModel {
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

fn tenant_from_model(model: tenants::Model) -> RepositoryResult<Tenant> {
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

fn format_timestamp(value: OffsetDateTime) -> RepositoryResult<String> {
    value
        .format(&Rfc3339)
        .context("failed to format onboarding timestamp")
        .map_err(RepositoryError::from)
}

fn user_external_projection_audit_event(
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

fn tenant_self_create_audit_event(tenant: &Tenant, actor: AuditActor) -> AuditEvent {
    record_audit_event(
        tenant.id,
        actor,
        "tenant.self_create",
        "tenant",
        Some(tenant.id.to_string()),
        json!({ "tenant_slug": tenant.slug }),
    )
}

fn join_link_audit_event(
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
