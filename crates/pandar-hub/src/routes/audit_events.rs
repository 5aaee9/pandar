use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    AppState,
    repositories::AuditEventListQuery,
    routes::{ApiError, auth, parse_tenant_id},
};

#[derive(Debug, Deserialize)]
pub(in crate::routes) struct AuditEventsQuery {
    limit: Option<usize>,
    before: Option<String>,
    action: Option<String>,
}

#[derive(Debug, Serialize)]
struct AuditEventResponse {
    id: String,
    tenant_id: String,
    actor_type: String,
    user_id: Option<String>,
    action: String,
    target_type: String,
    target_id: Option<String>,
    metadata: Value,
    created_at: String,
}

#[derive(Debug, Serialize)]
pub(in crate::routes) struct AuditEventListResponse {
    audit_events: Vec<AuditEventResponse>,
}

pub(in crate::routes) async fn list_audit_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    Query(query): Query<AuditEventsQuery>,
) -> Result<Json<AuditEventListResponse>, ApiError> {
    let tenant_id = parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant_admin_principal(&state, &headers, tenant_id).await?;
    let limit = query.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("invalid_limit"));
    }
    if let Some(before) = &query.before {
        OffsetDateTime::parse(before, &Rfc3339)
            .map_err(|_| ApiError::bad_request("invalid_before"))?;
    }

    let audit_events = state
        .audit_events()
        .list_for_tenant_query(
            tenant_id,
            AuditEventListQuery {
                limit: limit as u64,
                before: query.before,
                action: query.action,
            },
        )
        .await?
        .into_iter()
        .map(AuditEventResponse::from)
        .collect();

    Ok(Json(AuditEventListResponse { audit_events }))
}

fn audit_metadata(metadata_json: &str, event_id: &str) -> Value {
    let metadata = match serde_json::from_str::<Value>(metadata_json) {
        Ok(Value::Object(map)) => Value::Object(map),
        Ok(_) => Value::Object(Map::new()),
        Err(err) => {
            let error = anyhow::Error::new(err).context(format!(
                "failed to parse audit metadata for event {event_id}"
            ));
            tracing::error!(error = %format!("{error:#}"), "invalid persisted audit metadata");
            Value::Object(Map::new())
        }
    };
    redact_audit_metadata(metadata)
}

fn redact_audit_metadata(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter_map(|(key, value)| {
                    if is_forbidden_audit_metadata_key(&key) {
                        None
                    } else {
                        Some((key, redact_audit_metadata(value)))
                    }
                })
                .collect(),
        ),
        Value::Array(values) => {
            Value::Array(values.into_iter().map(redact_audit_metadata).collect())
        }
        other => other,
    }
}

fn is_forbidden_audit_metadata_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace(['-', ' '], "_");
    normalized.contains("plaintext_token")
        || normalized.contains("token_hash")
        || normalized.contains("agent_credential")
        || normalized.contains("credential_hash")
        || normalized.contains("plugin_ticket")
        || normalized == "ticket"
        || normalized.ends_with("_ticket")
        || normalized.contains("plaintext_ticket")
        || normalized.contains("ticket_hash")
        || normalized.contains("bambu_access_code")
        || normalized.contains("artifact_storage_path")
        || normalized == "storage_path"
        || normalized.contains("bearer")
        || normalized.contains("authorization")
        || normalized == "subject"
        || normalized.contains("external_subject")
        || normalized.contains("provider_subject")
        || normalized.contains("external_provider_subject")
}

impl From<crate::repositories::AuditEvent> for AuditEventResponse {
    fn from(event: crate::repositories::AuditEvent) -> Self {
        let metadata = audit_metadata(&event.metadata_json, &event.id);
        Self {
            id: event.id,
            tenant_id: event.tenant_id.to_string(),
            actor_type: event.actor_type,
            user_id: event.user_id,
            action: event.action,
            target_type: event.target_type,
            target_id: event.target_id,
            metadata,
            created_at: event.created_at,
        }
    }
}
