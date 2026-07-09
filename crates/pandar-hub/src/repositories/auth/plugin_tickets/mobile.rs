use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use sea_orm::TransactionTrait;

use super::{
    PLUGIN_LOGIN_TICKET_PREFIX, PluginLoginTicket, PluginLoginTicketWithPlaintext,
    insert_plugin_login_ticket, plugin_login_ticket_audit_event,
};
use crate::repositories::{
    AuditActor, AuthRepository, RepositoryError, RepositoryResult,
    audit::insert_audit_event_tx,
    auth::{hash_token, secrets::generate_secret},
};

impl AuthRepository {
    pub async fn create_mobile_login_ticket_with_audit(
        &self,
        tenant_id: TenantId,
        user_id: Option<String>,
        redirect_url: impl AsRef<str>,
        expires_at: String,
        actor: AuditActor,
    ) -> RepositoryResult<PluginLoginTicketWithPlaintext> {
        let redirect_url = self.validate_mobile_redirect_url(redirect_url.as_ref())?;
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
            .context("failed to begin mobile login ticket create transaction")?;
        insert_plugin_login_ticket(&tx, &ticket, &ticket_hash).await?;
        insert_audit_event_tx(
            &tx,
            &plugin_login_ticket_audit_event(&ticket, "mobile_login_ticket.create", actor, None),
        )
        .await?;
        tx.commit()
            .await
            .context("failed to commit mobile login ticket create transaction")?;

        Ok(PluginLoginTicketWithPlaintext {
            ticket,
            plaintext_ticket,
        })
    }

    pub fn validate_mobile_redirect_url(
        &self,
        redirect_url: impl AsRef<str>,
    ) -> RepositoryResult<String> {
        let redirect_url = redirect_url.as_ref();
        let uri = reqwest::Url::parse(redirect_url)
            .map_err(|_| RepositoryError::InvalidPluginRedirectUrl)?;
        if uri.scheme() != "zip.iptables.pandar.android"
            || uri.host_str().is_some()
            || uri.path() != "/auth/callback"
            || uri.query().is_some()
            || redirect_url.contains('#')
        {
            return Err(RepositoryError::InvalidPluginRedirectUrl);
        }

        Ok(redirect_url.to_owned())
    }
}
