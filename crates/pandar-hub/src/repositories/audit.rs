use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use serde::Serialize;
use sqlx::Row;

use crate::{db::Database, repositories::RepositoryResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditEvent {
    pub id: String,
    pub tenant_id: TenantId,
    pub actor_type: String,
    pub user_id: Option<String>,
    pub action: String,
    pub target_type: String,
    pub target_id: Option<String>,
    pub metadata_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordAuditEvent {
    pub tenant_id: TenantId,
    pub actor_type: String,
    pub user_id: Option<String>,
    pub action: String,
    pub target_type: String,
    pub target_id: Option<String>,
    pub metadata_json: String,
}

#[derive(Debug, Clone)]
pub struct AuditEventRepository {
    database: Database,
}

impl AuditEventRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn record(&self, event: RecordAuditEvent) -> RepositoryResult<AuditEvent> {
        let event = AuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: event.tenant_id,
            actor_type: event.actor_type,
            user_id: event.user_id,
            action: event.action,
            target_type: event.target_type,
            target_id: event.target_id,
            metadata_json: event.metadata_json,
            created_at: created_at_now(),
        };

        match &self.database {
            Database::Sqlite(pool) => {
                insert_audit_event_sqlite(pool, &event).await?;
            }
            Database::Postgres(pool) => {
                insert_audit_event_postgres(pool, &event).await?;
            }
        }

        Ok(event)
    }

    pub async fn list_for_tenant(&self, tenant_id: TenantId) -> RepositoryResult<Vec<AuditEvent>> {
        match &self.database {
            Database::Sqlite(pool) => {
                let rows = sqlx::query(
                    "SELECT id, tenant_id, actor_type, user_id, action, target_type, target_id, metadata_json, created_at
                     FROM audit_events
                     WHERE tenant_id = ?1
                     ORDER BY created_at ASC, id ASC",
                )
                .bind(tenant_id.to_string())
                .fetch_all(pool)
                .await
                .context("failed to list SQLite audit events")?;
                rows.into_iter()
                    .map(|row| {
                        audit_event_from_parts(AuditEventParts {
                            id: row.get("id"),
                            tenant_id: row.get("tenant_id"),
                            actor_type: row.get("actor_type"),
                            user_id: row.get("user_id"),
                            action: row.get("action"),
                            target_type: row.get("target_type"),
                            target_id: row.get("target_id"),
                            metadata_json: row.get("metadata_json"),
                            created_at: row.get("created_at"),
                        })
                    })
                    .collect()
            }
            Database::Postgres(pool) => {
                let rows = sqlx::query(
                    "SELECT id, tenant_id, actor_type, user_id, action, target_type, target_id, metadata_json, created_at
                     FROM audit_events
                     WHERE tenant_id = $1
                     ORDER BY created_at ASC, id ASC",
                )
                .bind(tenant_id.to_string())
                .fetch_all(pool)
                .await
                .context("failed to list PostgreSQL audit events")?;
                rows.into_iter()
                    .map(|row| {
                        audit_event_from_parts(AuditEventParts {
                            id: row.get("id"),
                            tenant_id: row.get("tenant_id"),
                            actor_type: row.get("actor_type"),
                            user_id: row.get("user_id"),
                            action: row.get("action"),
                            target_type: row.get("target_type"),
                            target_id: row.get("target_id"),
                            metadata_json: row.get("metadata_json"),
                            created_at: row.get("created_at"),
                        })
                    })
                    .collect()
            }
        }
    }
}

pub(crate) async fn insert_audit_event_sqlite<'e, E>(
    executor: E,
    event: &AuditEvent,
) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query(
        "INSERT INTO audit_events (id, tenant_id, actor_type, user_id, action, target_type, target_id, metadata_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(&event.id)
    .bind(event.tenant_id.to_string())
    .bind(&event.actor_type)
    .bind(&event.user_id)
    .bind(&event.action)
    .bind(&event.target_type)
    .bind(&event.target_id)
    .bind(&event.metadata_json)
    .bind(&event.created_at)
    .execute(executor)
    .await
    .context("failed to insert SQLite audit event")?;
    Ok(())
}

pub(crate) async fn insert_audit_event_postgres<'e, E>(
    executor: E,
    event: &AuditEvent,
) -> RepositoryResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query(
        "INSERT INTO audit_events (id, tenant_id, actor_type, user_id, action, target_type, target_id, metadata_json, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(&event.id)
    .bind(event.tenant_id.to_string())
    .bind(&event.actor_type)
    .bind(&event.user_id)
    .bind(&event.action)
    .bind(&event.target_type)
    .bind(&event.target_id)
    .bind(&event.metadata_json)
    .bind(&event.created_at)
    .execute(executor)
    .await
    .context("failed to insert PostgreSQL audit event")?;
    Ok(())
}

pub(crate) fn build_audit_event(event: RecordAuditEvent) -> AuditEvent {
    AuditEvent {
        id: uuid::Uuid::new_v4().to_string(),
        tenant_id: event.tenant_id,
        actor_type: event.actor_type,
        user_id: event.user_id,
        action: event.action,
        target_type: event.target_type,
        target_id: event.target_id,
        metadata_json: event.metadata_json,
        created_at: created_at_now(),
    }
}

struct AuditEventParts {
    id: String,
    tenant_id: String,
    actor_type: String,
    user_id: Option<String>,
    action: String,
    target_type: String,
    target_id: Option<String>,
    metadata_json: String,
    created_at: String,
}

fn audit_event_from_parts(parts: AuditEventParts) -> RepositoryResult<AuditEvent> {
    Ok(AuditEvent {
        id: parts.id,
        tenant_id: TenantId::parse(&parts.tenant_id).map_err(anyhow::Error::from)?,
        actor_type: parts.actor_type,
        user_id: parts.user_id,
        action: parts.action,
        target_type: parts.target_type,
        target_id: parts.target_id,
        metadata_json: parts.metadata_json,
        created_at: parts.created_at,
    })
}
