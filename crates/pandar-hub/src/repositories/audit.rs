use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseTransaction,
    EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};
use serde::Serialize;
use serde_json::{Map, Value};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditActor {
    pub actor_type: String,
    pub user_id: Option<String>,
    pub metadata: Option<Value>,
}

impl AuditActor {
    pub fn user(user_id: impl Into<String>) -> Self {
        Self {
            actor_type: "user".to_owned(),
            user_id: Some(user_id.into()),
            metadata: None,
        }
    }

    pub fn tenant_token(
        user_id: Option<String>,
        tenant_token_id: impl Into<String>,
        tenant_token_scopes: Vec<&'static str>,
    ) -> Self {
        Self {
            actor_type: "tenant_token".to_owned(),
            user_id,
            metadata: Some(serde_json::json!({
                "tenant_token_id": tenant_token_id.into(),
                "tenant_token_scopes": tenant_token_scopes,
            })),
        }
    }

    pub fn plugin_token(
        user_id: Option<String>,
        tenant_token_id: impl Into<String>,
        tenant_token_scopes: Vec<&'static str>,
    ) -> Self {
        Self {
            actor_type: "plugin_token".to_owned(),
            user_id,
            metadata: Some(serde_json::json!({
                "tenant_token_id": tenant_token_id.into(),
                "tenant_token_scopes": tenant_token_scopes,
            })),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEventListQuery {
    pub limit: u64,
    pub before: Option<String>,
    pub action: Option<String>,
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

    pub async fn list_for_tenant_query(
        &self,
        tenant_id: TenantId,
        query: AuditEventListQuery,
    ) -> RepositoryResult<Vec<AuditEvent>> {
        let limit = query.limit.clamp(1, 100);
        let mut select = audit_events::Entity::find()
            .filter(audit_events::Column::TenantId.eq(tenant_id.to_string()))
            .order_by_desc(audit_events::Column::CreatedAt)
            .order_by_desc(audit_events::Column::Id)
            .limit(limit);
        if let Some(before) = query.before {
            select = select.filter(audit_events::Column::CreatedAt.lt(before));
        }
        if let Some(action) = query.action {
            select = select.filter(audit_events::Column::Action.eq(action));
        }

        select
            .all(&self.database.sea_orm_connection())
            .await
            .context("failed to list audit events")?
            .into_iter()
            .map(audit_event_from_model)
            .collect()
    }

    pub async fn list_for_tenant_newest_first(
        &self,
        tenant_id: TenantId,
        limit: u64,
        before: Option<String>,
        action: Option<String>,
    ) -> RepositoryResult<Vec<AuditEvent>> {
        self.list_for_tenant_query(
            tenant_id,
            AuditEventListQuery {
                limit,
                before,
                action,
            },
        )
        .await
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

pub(crate) fn record_audit_event(
    tenant_id: TenantId,
    actor: AuditActor,
    action: impl Into<String>,
    target_type: impl Into<String>,
    target_id: Option<String>,
    metadata: Value,
) -> AuditEvent {
    build_audit_event(RecordAuditEvent {
        tenant_id,
        actor_type: actor.actor_type,
        user_id: actor.user_id,
        action: action.into(),
        target_type: target_type.into(),
        target_id,
        metadata_json: merge_actor_metadata(metadata, actor.metadata).to_string(),
    })
}

fn merge_actor_metadata(metadata: Value, actor_metadata: Option<Value>) -> Value {
    let Some(actor_metadata) = actor_metadata else {
        return metadata;
    };

    let mut merged = match metadata {
        Value::Object(map) => map,
        other => {
            let mut map = Map::new();
            map.insert("metadata".to_owned(), other);
            map
        }
    };
    if let Value::Object(actor_map) = actor_metadata {
        merged.extend(actor_map);
    }
    Value::Object(merged)
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
