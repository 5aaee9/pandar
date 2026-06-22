use anyhow::Context;
use pandar_core::{Agent, TenantId};
use serde_json::json;

use crate::{
    db::Database,
    repositories::{
        AgentRepository, RepositoryResult,
        agents::{insert_agent_postgres, insert_agent_sqlite, sqlx_err_to_repo},
        audit::{build_audit_event, insert_audit_event_postgres, insert_audit_event_sqlite},
    },
};

impl AgentRepository {
    pub async fn create_pairing_bundle_with_audit(
        &self,
        tenant_id: TenantId,
        name: impl Into<String>,
        user_id: String,
    ) -> RepositoryResult<Agent> {
        let agent = Agent::new(tenant_id, name).map_err(anyhow::Error::from)?;
        match &self.database {
            Database::Sqlite(pool) => {
                let mut tx = pool
                    .begin()
                    .await
                    .context("failed to begin SQLite agent pairing audit transaction")?;
                let inserted = insert_agent_sqlite(&mut *tx, &agent)
                    .await
                    .map_err(sqlx_err_to_repo);
                match inserted {
                    Ok(()) => {
                        insert_audit_event_sqlite(&mut *tx, &pairing_event(&agent, user_id))
                            .await?;
                        tx.commit()
                            .await
                            .context("failed to commit SQLite agent pairing audit transaction")?;
                        Ok(agent)
                    }
                    Err(err) => {
                        tx.rollback().await.context(
                            "failed to roll back SQLite agent pairing audit transaction",
                        )?;
                        Err(err)
                    }
                }
            }
            Database::Postgres(pool) => {
                let mut tx = pool
                    .begin()
                    .await
                    .context("failed to begin PostgreSQL agent pairing audit transaction")?;
                let inserted = insert_agent_postgres(&mut *tx, &agent)
                    .await
                    .map_err(sqlx_err_to_repo);
                match inserted {
                    Ok(()) => {
                        insert_audit_event_postgres(&mut *tx, &pairing_event(&agent, user_id))
                            .await?;
                        tx.commit().await.context(
                            "failed to commit PostgreSQL agent pairing audit transaction",
                        )?;
                        Ok(agent)
                    }
                    Err(err) => {
                        tx.rollback().await.context(
                            "failed to roll back PostgreSQL agent pairing audit transaction",
                        )?;
                        Err(err)
                    }
                }
            }
        }
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
