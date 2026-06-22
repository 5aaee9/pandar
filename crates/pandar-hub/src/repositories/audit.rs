use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseTransaction,
    EntityTrait, QueryFilter, QueryOrder,
};
use serde::Serialize;

use crate::{db::Database, entities::audit_events, repositories::RepositoryResult};

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
        let event = build_audit_event(event);
        insert_audit_event(&self.database.sea_orm_connection(), &event).await?;

        Ok(event)
    }

    pub async fn list_for_tenant(&self, tenant_id: TenantId) -> RepositoryResult<Vec<AuditEvent>> {
        audit_events::Entity::find()
            .filter(audit_events::Column::TenantId.eq(tenant_id.to_string()))
            .order_by_asc(audit_events::Column::CreatedAt)
            .order_by_asc(audit_events::Column::Id)
            .all(&self.database.sea_orm_connection())
            .await
            .context("failed to list audit events")?
            .into_iter()
            .map(audit_event_from_model)
            .collect()
    }
}

pub(crate) async fn insert_audit_event<C>(
    connection: &C,
    event: &AuditEvent,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    audit_model(event)
        .insert(connection)
        .await
        .context("failed to insert audit event")?;
    Ok(())
}

pub(crate) async fn insert_audit_event_tx(
    tx: &DatabaseTransaction,
    event: &AuditEvent,
) -> RepositoryResult<()> {
    insert_audit_event(tx, event).await
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

fn audit_event_from_model(model: audit_events::Model) -> RepositoryResult<AuditEvent> {
    Ok(AuditEvent {
        id: model.id,
        tenant_id: TenantId::parse(&model.tenant_id).map_err(anyhow::Error::from)?,
        actor_type: model.actor_type,
        user_id: model.user_id,
        action: model.action,
        target_type: model.target_type,
        target_id: model.target_id,
        metadata_json: model.metadata_json,
        created_at: model.created_at,
    })
}

fn audit_model(event: &AuditEvent) -> audit_events::ActiveModel {
    audit_events::ActiveModel {
        id: Set(event.id.clone()),
        tenant_id: Set(event.tenant_id.to_string()),
        actor_type: Set(event.actor_type.clone()),
        user_id: Set(event.user_id.clone()),
        action: Set(event.action.clone()),
        target_type: Set(event.target_type.clone()),
        target_id: Set(event.target_id.clone()),
        metadata_json: Set(event.metadata_json.clone()),
        created_at: Set(event.created_at.clone()),
    }
}
