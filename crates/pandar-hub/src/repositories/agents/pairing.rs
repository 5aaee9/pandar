use anyhow::Context;
use pandar_core::{Agent, TenantId};
use sea_orm::TransactionTrait;
use serde_json::json;

use crate::repositories::{
    AgentRepository, RepositoryResult,
    agents::insert_agent,
    audit::{build_audit_event, insert_audit_event_tx},
};

impl AgentRepository {
    pub async fn create_pairing_bundle_with_audit(
        &self,
        tenant_id: TenantId,
        name: impl Into<String>,
        user_id: String,
    ) -> RepositoryResult<Agent> {
        let agent = Agent::new(tenant_id, name).map_err(anyhow::Error::from)?;
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin()
            .await
            .context("failed to begin agent pairing audit transaction")?;
        insert_agent(&tx, &agent).await?;
        insert_audit_event_tx(&tx, &pairing_event(&agent, user_id)).await?;
        tx.commit()
            .await
            .context("failed to commit agent pairing audit transaction")?;

        Ok(agent)
    }
}

fn pairing_event(agent: &Agent, user_id: String) -> crate::repositories::AuditEvent {
    build_audit_event(crate::repositories::RecordAuditEvent {
        tenant_id: agent.tenant_id,
        actor_type: "user".to_owned(),
        user_id: Some(user_id),
        action: "agent.pairing_bundle".to_owned(),
        target_type: "agent".to_owned(),
        target_id: Some(agent.id.to_string()),
        metadata_json: json!({ "agent_name": agent.name }).to_string(),
    })
}
