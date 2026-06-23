use anyhow::Context;
use pandar_core::{Agent, TenantId};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, TransactionTrait};
use serde_json::json;

use crate::repositories::{
    AgentRepository, AuditActor, RepositoryError, RepositoryResult,
    audit::{insert_audit_event_tx, record_audit_event},
    auth::{hash_token, secrets::generate_secret},
    is_sea_orm_foreign_key_violation, is_sea_orm_unique_violation,
};

pub const AGENT_CREDENTIAL_PREFIX: &str = "pandar_ac_";

pub struct AgentPairingBundle {
    pub agent: Agent,
    pub credential: String,
}

impl AgentRepository {
    pub async fn create_pairing_bundle_with_audit(
        &self,
        tenant_id: TenantId,
        name: impl Into<String>,
        actor: AuditActor,
    ) -> RepositoryResult<AgentPairingBundle> {
        let agent = Agent::new(tenant_id, name).map_err(anyhow::Error::from)?;
        let credential = generate_secret(AGENT_CREDENTIAL_PREFIX);
        let credential_hash = hash_token(&credential);
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin agent pairing audit transaction")?;
        let insert_result = crate::entities::agents::ActiveModel {
            id: Set(agent.id.to_string()),
            tenant_id: Set(agent.tenant_id.to_string()),
            name: Set(agent.name.clone()),
            status: Set(agent.status.as_str().to_owned()),
            version: Set(None),
            last_seen_at: Set(None),
            created_at: Set(agent.created_at.clone()),
            credential_hash: Set(Some(credential_hash)),
            credential_rotated_at: Set(Some(pandar_core::created_at_now())),
            credential_revoked_at: Set(None),
        }
        .insert(&tx)
        .await
        .map(|_| ());
        match insert_result {
            Ok(()) => {}
            Err(err)
                if is_sea_orm_unique_violation(
                    &err,
                    "agents.tenant_id, agents.name",
                    "agents_tenant_id_name_key",
                ) =>
            {
                return Err(RepositoryError::DuplicateAgentName);
            }
            Err(err) if is_sea_orm_foreign_key_violation(&err) => {
                return Err(RepositoryError::MissingTenant);
            }
            Err(err) => {
                return Err(anyhow::Error::new(err)
                    .context("failed to insert paired agent")
                    .into());
            }
        }
        insert_audit_event_tx(&tx, &pairing_event(&agent, actor)).await?;
        tx.commit()
            .await
            .context("failed to commit agent pairing audit transaction")?;

        Ok(AgentPairingBundle { agent, credential })
    }
}

fn pairing_event(agent: &Agent, actor: AuditActor) -> crate::repositories::AuditEvent {
    record_audit_event(
        agent.tenant_id,
        actor,
        "agent.pairing_bundle",
        "agent",
        Some(agent.id.to_string()),
        json!({ "agent_name": agent.name }),
    )
}
