use anyhow::Context;
use pandar_core::{Agent, AgentId, AgentStatus, TenantId};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, TransactionTrait,
};

mod pairing;

pub use pairing::AGENT_CREDENTIAL_PREFIX;

use crate::{
    db::Database,
    entities::{agents, tenants},
    repositories::{
        AuditActor, RepositoryError, RepositoryResult,
        audit::{insert_audit_event_tx, record_audit_event},
        auth::hash_token,
        is_sea_orm_foreign_key_violation, is_sea_orm_unique_violation,
    },
};

#[derive(Debug, Clone)]
pub struct AgentRepository {
    database: Database,
}

#[derive(Debug, Clone)]
pub struct AgentCredentialRecord {
    pub agent: Agent,
    pub credential_hash: Option<String>,
    pub credential_rotated_at: Option<String>,
    pub credential_revoked_at: Option<String>,
}

impl AgentRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn create(
        &self,
        tenant_id: TenantId,
        name: impl Into<String>,
    ) -> RepositoryResult<Agent> {
        let agent = Agent::new(tenant_id, name).map_err(anyhow::Error::from)?;
        self.insert_agent(agent).await
    }

    pub async fn create_with_audit(
        &self,
        tenant_id: TenantId,
        name: impl Into<String>,
        actor: AuditActor,
    ) -> RepositoryResult<Agent> {
        let agent = Agent::new(tenant_id, name).map_err(anyhow::Error::from)?;
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin agent create audit transaction")?;
        insert_agent(&tx, &agent).await?;
        let event = record_audit_event(
            tenant_id,
            actor,
            "agent.create",
            "agent",
            Some(agent.id.to_string()),
            serde_json::json!({}),
        );
        insert_audit_event_tx(&tx, &event).await?;
        tx.commit()
            .await
            .context("failed to commit agent create audit transaction")?;

        Ok(agent)
    }

    async fn insert_agent(&self, agent: Agent) -> RepositoryResult<Agent> {
        insert_agent(&self.database.sea_orm_connection(), &agent).await?;
        Ok(agent)
    }

    pub async fn list_for_tenant(&self, tenant_id: TenantId) -> RepositoryResult<Vec<Agent>> {
        let connection = self.database.sea_orm_connection();
        if !tenant_exists(&connection, tenant_id).await? {
            return Err(RepositoryError::MissingTenant);
        }

        agents::Entity::find()
            .filter(agents::Column::TenantId.eq(tenant_id.to_string()))
            .order_by_asc(agents::Column::CreatedAt)
            .order_by_asc(agents::Column::Id)
            .all(&connection)
            .await
            .context("failed to list agents")?
            .into_iter()
            .map(agent_from_model)
            .collect()
    }

    pub async fn get(&self, agent_id: AgentId) -> RepositoryResult<Option<Agent>> {
        agents::Entity::find_by_id(agent_id.to_string())
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to get agent")?
            .map(agent_from_model)
            .transpose()
    }

    pub async fn get_credential_record(
        &self,
        agent_id: AgentId,
    ) -> RepositoryResult<Option<AgentCredentialRecord>> {
        agents::Entity::find_by_id(agent_id.to_string())
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to get agent credential")?
            .map(agent_credential_from_model)
            .transpose()
    }

    pub async fn credential_records_by_hash(
        &self,
        credential_hash: &str,
    ) -> RepositoryResult<Vec<AgentCredentialRecord>> {
        agents::Entity::find()
            .filter(agents::Column::CredentialHash.eq(credential_hash))
            .order_by_asc(agents::Column::Id)
            .all(&self.database.sea_orm_connection())
            .await
            .context("failed to get agent credentials by hash")?
            .into_iter()
            .map(agent_credential_from_model)
            .collect()
    }

    pub async fn update_connection(
        &self,
        agent_id: AgentId,
        status: AgentStatus,
        version: Option<&str>,
        last_seen_at: &str,
    ) -> RepositoryResult<Agent> {
        let connection = self.database.sea_orm_connection();
        let Some(agent) = agents::Entity::find_by_id(agent_id.to_string())
            .one(&connection)
            .await
            .context("failed to get agent before connection update")?
        else {
            return Err(RepositoryError::MissingAgent);
        };

        let mut active: agents::ActiveModel = agent.into();
        active.status = Set(status.as_str().to_owned());
        if let Some(version) = version {
            active.version = Set(Some(version.to_owned()));
        }
        active.last_seen_at = Set(Some(last_seen_at.to_owned()));
        active
            .update(&connection)
            .await
            .context("failed to update agent connection")
            .map_err(Into::into)
            .and_then(agent_from_model)
    }

    pub async fn mark_offline(
        &self,
        agent_id: AgentId,
        last_seen_at: &str,
    ) -> RepositoryResult<Agent> {
        self.update_connection(agent_id, AgentStatus::Offline, None, last_seen_at)
            .await
    }

    pub async fn count(&self) -> RepositoryResult<i64> {
        let count = agents::Entity::find()
            .count(&self.database.sea_orm_connection())
            .await
            .context("failed to count agents")?;

        Ok(count.try_into().expect("agent count should fit in i64"))
    }

    pub async fn rotate_credential(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        credential: &str,
        actor: AuditActor,
    ) -> RepositoryResult<AgentCredentialRecord> {
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin agent credential rotation transaction")?;
        let Some(agent) = agents::Entity::find_by_id(agent_id.to_string())
            .one(&tx)
            .await
            .context("failed to get agent before credential rotation")?
        else {
            return Err(RepositoryError::MissingAgent);
        };
        if agent.tenant_id != tenant_id.to_string() {
            return Err(RepositoryError::MissingAgent);
        }

        let mut active: agents::ActiveModel = agent.into();
        active.credential_hash = Set(Some(hash_token(credential)));
        active.credential_rotated_at = Set(Some(pandar_core::created_at_now()));
        active.credential_revoked_at = Set(None);
        let updated = active
            .update(&tx)
            .await
            .context("failed to rotate agent credential")
            .map_err(RepositoryError::from)
            .and_then(agent_credential_from_model)?;
        insert_audit_event_tx(
            &tx,
            &record_audit_event(
                tenant_id,
                actor,
                "agent.credential_rotate",
                "agent",
                Some(agent_id.to_string()),
                serde_json::json!({}),
            ),
        )
        .await?;
        tx.commit()
            .await
            .context("failed to commit agent credential rotation transaction")?;

        Ok(updated)
    }

    pub async fn revoke_credential(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        actor: AuditActor,
    ) -> RepositoryResult<AgentCredentialRecord> {
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin agent credential revocation transaction")?;
        let Some(agent) = agents::Entity::find_by_id(agent_id.to_string())
            .one(&tx)
            .await
            .context("failed to get agent before credential revocation")?
        else {
            return Err(RepositoryError::MissingAgent);
        };
        if agent.tenant_id != tenant_id.to_string() {
            return Err(RepositoryError::MissingAgent);
        }

        let mut active: agents::ActiveModel = agent.into();
        active.credential_revoked_at = Set(Some(pandar_core::created_at_now()));
        let updated = active
            .update(&tx)
            .await
            .context("failed to revoke agent credential")
            .map_err(RepositoryError::from)
            .and_then(agent_credential_from_model)?;
        insert_audit_event_tx(
            &tx,
            &record_audit_event(
                tenant_id,
                actor,
                "agent.credential_revoke",
                "agent",
                Some(agent_id.to_string()),
                serde_json::json!({}),
            ),
        )
        .await?;
        tx.commit()
            .await
            .context("failed to commit agent credential revocation transaction")?;

        Ok(updated)
    }
}

pub(super) async fn insert_agent<C>(connection: &C, agent: &Agent) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    let result = agents::ActiveModel {
        id: Set(agent.id.to_string()),
        tenant_id: Set(agent.tenant_id.to_string()),
        name: Set(agent.name.clone()),
        status: Set(agent.status.as_str().to_owned()),
        version: Set(None),
        last_seen_at: Set(None),
        created_at: Set(agent.created_at.clone()),
        credential_hash: Set(None),
        credential_rotated_at: Set(None),
        credential_revoked_at: Set(None),
    }
    .insert(connection)
    .await
    .map(|_| ());

    match result {
        Ok(()) => Ok(()),
        Err(err)
            if is_sea_orm_unique_violation(
                &err,
                "agents.tenant_id, agents.name",
                "agents_tenant_id_name_key",
            ) =>
        {
            Err(RepositoryError::DuplicateAgentName)
        }
        Err(err) if is_sea_orm_foreign_key_violation(&err) => Err(RepositoryError::MissingTenant),
        Err(err) => Err(anyhow::Error::new(err)
            .context("failed to insert agent")
            .into()),
    }
}

async fn tenant_exists<C>(connection: &C, tenant_id: TenantId) -> RepositoryResult<bool>
where
    C: ConnectionTrait,
{
    tenants::Entity::find_by_id(tenant_id.to_string())
        .one(connection)
        .await
        .context("failed to check tenant existence")
        .map(|tenant| tenant.is_some())
        .map_err(Into::into)
}

fn agent_from_model(model: agents::Model) -> RepositoryResult<Agent> {
    let status = model
        .status
        .parse::<AgentStatus>()
        .map_err(|_| RepositoryError::InvalidPersistedStatus(model.status.clone()))?;
    Agent::from_parts(
        AgentId::parse(&model.id).map_err(anyhow::Error::from)?,
        TenantId::parse(&model.tenant_id).map_err(anyhow::Error::from)?,
        model.name,
        status,
        model.created_at,
    )
    .map_err(anyhow::Error::from)
    .context("failed to rehydrate agent")
    .map_err(RepositoryError::from)
}

fn agent_credential_from_model(model: agents::Model) -> RepositoryResult<AgentCredentialRecord> {
    let credential_hash = model.credential_hash.clone();
    let credential_rotated_at = model.credential_rotated_at.clone();
    let credential_revoked_at = model.credential_revoked_at.clone();
    Ok(AgentCredentialRecord {
        agent: agent_from_model(model)?,
        credential_hash,
        credential_rotated_at,
        credential_revoked_at,
    })
}
