use anyhow::Context;
use axum::http::Uri;
use pandar_core::{TenantId, created_at_now};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    TransactionTrait,
};
use serde::Serialize;
use serde_json::json;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    entities::plugin_login_tickets,
    repositories::{
        AuditActor, AuditEvent, AuthRepository, RepositoryError, RepositoryResult,
        TenantTokenWithPlaintext,
        audit::{insert_audit_event_tx, record_audit_event},
        auth::{hash_token, secrets::generate_secret, user_exists},
        is_sea_orm_foreign_key_violation, is_sea_orm_unique_violation,
    },
};

const PLUGIN_LOGIN_TICKET_PREFIX: &str = "pandar_plugin_ticket_";
const PLUGIN_TOKEN_SCOPE: &str = "plugin:studio";
const PLUGIN_TOKEN_TTL_DAYS: i64 = 30;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginLoginTicket {
    pub id: String,
    pub tenant_id: TenantId,
    pub user_id: Option<String>,
    pub redirect_url: String,
    pub created_at: String,
    pub expires_at: String,
    pub used_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLoginTicketWithPlaintext {
    pub ticket: PluginLoginTicket,
    pub plaintext_ticket: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLoginTicketExchange {
    pub ticket: PluginLoginTicket,
    pub redirect_url: String,
    pub tenant_token: TenantTokenWithPlaintext,
}

impl AuthRepository {
    pub async fn create_plugin_login_ticket_with_audit(
        &self,
        tenant_id: TenantId,
        user_id: Option<String>,
        redirect_url: impl AsRef<str>,
        expires_at: String,
        actor: AuditActor,
    ) -> RepositoryResult<PluginLoginTicketWithPlaintext> {
        let redirect_url = self.validate_plugin_redirect_url(redirect_url.as_ref())?;
        let plaintext_ticket = generate_secret(PLUGIN_LOGIN_TICKET_PREFIX);
        let ticket = PluginLoginTicket {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id,
            user_id,
            redirect_url,
            created_at: created_at_now(),
            expires_at,
            used_at: None,
            revoked_at: None,
        };
        let ticket_hash = hash_token(&plaintext_ticket);

        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin plugin login ticket create transaction")?;
        insert_plugin_login_ticket(&tx, &ticket, &ticket_hash).await?;
        insert_audit_event_tx(
            &tx,
            &plugin_login_ticket_audit_event(&ticket, "plugin_login_ticket.create", actor, None),
        )
        .await?;
        tx.commit()
            .await
            .context("failed to commit plugin login ticket create transaction")?;

        Ok(PluginLoginTicketWithPlaintext {
            ticket,
            plaintext_ticket,
        })
    }

    pub async fn exchange_plugin_login_ticket(
        &self,
        plaintext_ticket: &str,
    ) -> RepositoryResult<Option<PluginLoginTicketExchange>> {
        let ticket_hash = hash_token(plaintext_ticket);
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin plugin login ticket exchange transaction")?;

        let Some(model) = plugin_login_tickets::Entity::find()
            .filter(plugin_login_tickets::Column::TicketHash.eq(ticket_hash))
            .filter(plugin_login_tickets::Column::UsedAt.is_null())
            .filter(plugin_login_tickets::Column::RevokedAt.is_null())
            .one(&tx)
            .await
            .context("failed to load plugin login ticket for exchange")?
        else {
            return Ok(None);
        };
        let ticket = plugin_login_ticket_from_model(model)?;
        if plugin_login_ticket_expired(&ticket)? {
            return Ok(None);
        }

        let used_at = created_at_now();
        let result = plugin_login_tickets::Entity::update_many()
            .set(plugin_login_tickets::ActiveModel {
                used_at: Set(Some(used_at.clone())),
                ..Default::default()
            })
            .filter(plugin_login_tickets::Column::Id.eq(ticket.id.clone()))
            .filter(plugin_login_tickets::Column::UsedAt.is_null())
            .filter(plugin_login_tickets::Column::RevokedAt.is_null())
            .exec(&tx)
            .await
            .context("failed to mark plugin login ticket used")?;
        if result.rows_affected != 1 {
            return Ok(None);
        }

        let mut used_ticket = ticket.clone();
        used_ticket.used_at = Some(used_at);
        let token_expires_at = (OffsetDateTime::now_utc() + Duration::days(PLUGIN_TOKEN_TTL_DAYS))
            .format(&Rfc3339)
            .context("failed to format plugin tenant token expiry")?;
        let tenant_token = AuthRepository::create_plugin_token_from_ticket_tx(
            &tx,
            used_ticket.tenant_id,
            "Bambu Studio plugin",
            used_ticket.user_id.clone(),
            token_expires_at,
        )
        .await?;
        insert_audit_event_tx(
            &tx,
            &plugin_login_ticket_audit_event(
                &used_ticket,
                "plugin_login_ticket.exchange",
                AuditActor::plugin_token(
                    used_ticket.user_id.clone(),
                    tenant_token.token.id.clone(),
                    vec![PLUGIN_TOKEN_SCOPE],
                ),
                Some(tenant_token.token.id.clone()),
            ),
        )
        .await?;
        tx.commit()
            .await
            .context("failed to commit plugin login ticket exchange transaction")?;

        Ok(Some(PluginLoginTicketExchange {
            redirect_url: used_ticket.redirect_url.clone(),
            ticket: used_ticket,
            tenant_token,
        }))
    }

    pub fn validate_plugin_redirect_url(
        &self,
        redirect_url: impl AsRef<str>,
    ) -> RepositoryResult<String> {
        let redirect_url = redirect_url.as_ref();
        let uri = redirect_url
            .parse::<Uri>()
            .map_err(|_| RepositoryError::InvalidPluginRedirectUrl)?;
        if uri.scheme_str() != Some("http") || redirect_url.contains('#') {
            return Err(RepositoryError::InvalidPluginRedirectUrl);
        }
        let authority = uri
            .authority()
            .ok_or(RepositoryError::InvalidPluginRedirectUrl)?;
        if authority.as_str().contains('@') {
            return Err(RepositoryError::InvalidPluginRedirectUrl);
        }
        let host = authority.host();
        if !matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]") {
            return Err(RepositoryError::InvalidPluginRedirectUrl);
        }
        let Some(port) = authority.port_u16() else {
            return Err(RepositoryError::InvalidPluginRedirectUrl);
        };
        if port == 0 {
            return Err(RepositoryError::InvalidPluginRedirectUrl);
        }

        Ok(uri.to_string())
    }
}

async fn insert_plugin_login_ticket<C>(
    connection: &C,
    ticket: &PluginLoginTicket,
    ticket_hash: &str,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    if let Some(user_id) = &ticket.user_id {
        user_exists(
            connection,
            ticket.tenant_id,
            user_id,
            "failed to check plugin login ticket user",
        )
        .await?
        .then_some(())
        .ok_or(RepositoryError::MissingUser)?;
    }

    let result = plugin_login_ticket_model(ticket, ticket_hash)
        .insert(connection)
        .await
        .map(|_| ());
    match result {
        Ok(()) => Ok(()),
        Err(err)
            if is_sea_orm_unique_violation(
                &err,
                "plugin_login_tickets.ticket_hash",
                "plugin_login_tickets_ticket_hash_key",
            ) =>
        {
            Err(RepositoryError::DuplicatePluginLoginTicketHash)
        }
        Err(err) if is_sea_orm_foreign_key_violation(&err) => Err(RepositoryError::MissingTenant),
        Err(err) => Err(anyhow::Error::new(err)
            .context("failed to insert plugin login ticket")
            .into()),
    }
}

fn plugin_login_ticket_from_model(
    model: plugin_login_tickets::Model,
) -> RepositoryResult<PluginLoginTicket> {
    Ok(PluginLoginTicket {
        id: model.id,
        tenant_id: TenantId::parse(&model.tenant_id).map_err(anyhow::Error::from)?,
        user_id: model.user_id,
        redirect_url: model.redirect_url,
        created_at: model.created_at,
        expires_at: model.expires_at,
        used_at: model.used_at,
        revoked_at: model.revoked_at,
    })
}

fn plugin_login_ticket_model(
    ticket: &PluginLoginTicket,
    ticket_hash: &str,
) -> plugin_login_tickets::ActiveModel {
    plugin_login_tickets::ActiveModel {
        id: Set(ticket.id.clone()),
        tenant_id: Set(ticket.tenant_id.to_string()),
        user_id: Set(ticket.user_id.clone()),
        ticket_hash: Set(ticket_hash.to_owned()),
        redirect_url: Set(ticket.redirect_url.clone()),
        created_at: Set(ticket.created_at.clone()),
        expires_at: Set(ticket.expires_at.clone()),
        used_at: Set(ticket.used_at.clone()),
        revoked_at: Set(ticket.revoked_at.clone()),
    }
}

fn plugin_login_ticket_expired(ticket: &PluginLoginTicket) -> RepositoryResult<bool> {
    let expires_at = OffsetDateTime::parse(&ticket.expires_at, &Rfc3339).with_context(|| {
        format!(
            "failed to parse plugin login ticket expiry for {}",
            ticket.id
        )
    })?;
    Ok(expires_at <= OffsetDateTime::now_utc())
}

fn plugin_login_ticket_audit_event(
    ticket: &PluginLoginTicket,
    action: &'static str,
    actor: AuditActor,
    tenant_token_id: Option<String>,
) -> AuditEvent {
    record_audit_event(
        ticket.tenant_id,
        actor,
        action,
        "plugin_login_ticket",
        Some(ticket.id.clone()),
        json!({ "issued_tenant_token_id": tenant_token_id }),
    )
}
