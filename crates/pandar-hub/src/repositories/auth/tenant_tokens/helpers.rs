use anyhow::Context;
use sea_orm::ActiveValue::Set;
use serde::Serialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    entities::tenant_tokens,
    repositories::{
        AuditActor, AuditEvent, RepositoryResult,
        audit::{audit_metadata, record_audit_event},
        auth::tenant_tokens::{TenantToken, TenantTokenScope},
    },
};

#[derive(Serialize)]
struct TenantTokenAuditMetadata<'a> {
    name: &'a str,
}

pub(super) fn tenant_token_model(
    token: &TenantToken,
    token_hash: &str,
) -> tenant_tokens::ActiveModel {
    tenant_tokens::ActiveModel {
        id: Set(token.id.clone()),
        tenant_id: Set(token.tenant_id.to_string()),
        name: Set(token.name.clone()),
        token_hash: Set(token_hash.to_owned()),
        scopes_json: Set(scopes_json(&token.scopes)),
        created_by_user_id: Set(token.created_by_user_id.clone()),
        created_at: Set(token.created_at.clone()),
        last_used_at: Set(token.last_used_at.clone()),
        expires_at: Set(token.expires_at.clone()),
        revoked_at: Set(token.revoked_at.clone()),
    }
}

pub(super) fn scopes_json(scopes: &[TenantTokenScope]) -> String {
    serde_json::to_string(
        &scopes
            .iter()
            .map(|scope| scope.as_str())
            .collect::<Vec<_>>(),
    )
    .expect("tenant token scopes should serialize")
}

pub(super) fn is_expired(token: &TenantToken) -> RepositoryResult<bool> {
    let Some(expires_at) = &token.expires_at else {
        return Ok(false);
    };
    let expires_at = OffsetDateTime::parse(expires_at, &Rfc3339)
        .with_context(|| format!("failed to parse tenant token expiry for {}", token.id))?;
    Ok(expires_at <= OffsetDateTime::now_utc())
}

pub(super) fn tenant_token_audit_event(
    token: &TenantToken,
    action: &'static str,
    actor: AuditActor,
) -> AuditEvent {
    record_audit_event(
        token.tenant_id,
        actor,
        action,
        "tenant_token",
        Some(token.id.clone()),
        audit_metadata(TenantTokenAuditMetadata { name: &token.name }),
    )
}
