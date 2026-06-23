use anyhow::Context;
use pandar_core::{Tenant, created_at_now};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ConnectionTrait, TransactionTrait};
use serde_json::json;

use crate::{
    entities::tenants,
    repositories::{
        AuditEvent, AuthRepository, RepositoryError, RepositoryResult, TenantToken,
        TenantTokenScope, User, UserRole,
        audit::{build_audit_event, insert_audit_event_tx},
        auth::{insert_user, tenant_tokens::insert_tenant_token},
        is_sea_orm_unique_violation,
    },
};

const TENANT_TOKEN_PREFIX: &str = "pandar_tenant_";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrappedTenantAdmin {
    pub tenant: Tenant,
    pub user: User,
    pub tenant_token: TenantToken,
    pub plaintext_token: String,
}

impl AuthRepository {
    pub async fn bootstrap_tenant_admin_with_plaintext_token(
        &self,
        tenant_slug: impl Into<String>,
        tenant_display_name: impl Into<String>,
        admin_email: impl Into<String>,
        admin_display_name: impl Into<String>,
        api_token_name: impl Into<String>,
    ) -> RepositoryResult<BootstrappedTenantAdmin> {
        let tenant = Tenant::new(tenant_slug, tenant_display_name).map_err(anyhow::Error::from)?;
        let user = User {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: tenant.id,
            email: admin_email.into(),
            display_name: admin_display_name.into(),
            role: UserRole::TenantAdmin,
            created_at: created_at_now(),
        };
        let plaintext_token =
            crate::repositories::auth::secrets::generate_secret(TENANT_TOKEN_PREFIX);
        let tenant_token = TenantToken {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: tenant.id,
            name: api_token_name.into(),
            scopes: vec![TenantTokenScope::All],
            created_by_user_id: Some(user.id.clone()),
            created_at: created_at_now(),
            last_used_at: None,
            expires_at: None,
            revoked_at: None,
        };
        let token_hash = crate::repositories::auth::hash_token(&plaintext_token);

        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin bootstrap transaction")?;
        insert_tenant(&tx, &tenant).await?;
        insert_user(&tx, &user, "failed to insert bootstrap user").await?;
        insert_tenant_token(
            &tx,
            &tenant_token,
            &token_hash,
            "failed to insert bootstrap tenant token",
        )
        .await?;
        for event in bootstrap_audit_events(&tenant, &user, &tenant_token) {
            insert_audit_event_tx(&tx, &event).await?;
        }
        tx.commit()
            .await
            .context("failed to commit bootstrap transaction")?;

        Ok(BootstrappedTenantAdmin {
            tenant,
            user,
            tenant_token,
            plaintext_token,
        })
    }
}

async fn insert_tenant<C>(connection: &C, tenant: &Tenant) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    let result = tenants::ActiveModel {
        id: Set(tenant.id.to_string()),
        slug: Set(tenant.slug.clone()),
        display_name: Set(tenant.display_name.clone()),
        created_at: Set(tenant.created_at.clone()),
    }
    .insert(connection)
    .await
    .map(|_| ());

    match result {
        Ok(()) => Ok(()),
        Err(err) if is_sea_orm_unique_violation(&err, "tenants.slug", "tenants_slug_key") => {
            Err(RepositoryError::DuplicateTenantSlug)
        }
        Err(err) => Err(anyhow::Error::new(err)
            .context("failed to insert bootstrap tenant")
            .into()),
    }
}

fn bootstrap_audit_events(tenant: &Tenant, user: &User, token: &TenantToken) -> [AuditEvent; 3] {
    [
        build_audit_event(crate::repositories::RecordAuditEvent {
            tenant_id: tenant.id,
            actor_type: "bootstrap".to_owned(),
            user_id: None,
            action: "tenant.bootstrap".to_owned(),
            target_type: "tenant".to_owned(),
            target_id: Some(tenant.id.to_string()),
            metadata_json: json!({ "tenant_slug": tenant.slug }).to_string(),
        }),
        build_audit_event(crate::repositories::RecordAuditEvent {
            tenant_id: tenant.id,
            actor_type: "bootstrap".to_owned(),
            user_id: None,
            action: "user.create".to_owned(),
            target_type: "user".to_owned(),
            target_id: Some(user.id.clone()),
            metadata_json: json!({ "email": user.email, "role": user.role.as_str() }).to_string(),
        }),
        build_audit_event(crate::repositories::RecordAuditEvent {
            tenant_id: tenant.id,
            actor_type: "bootstrap".to_owned(),
            user_id: None,
            action: "tenant_token.create".to_owned(),
            target_type: "tenant_token".to_owned(),
            target_id: Some(token.id.clone()),
            metadata_json:
                json!({ "name": token.name, "created_by_user_id": token.created_by_user_id })
                    .to_string(),
        }),
    ]
}
