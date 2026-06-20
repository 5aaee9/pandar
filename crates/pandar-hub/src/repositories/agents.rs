use anyhow::Context;
use pandar_core::{Agent, AgentId, AgentStatus, TenantId};
use sqlx::Row;

use crate::{
    db::Database,
    repositories::{
        RepositoryError, RepositoryResult, is_foreign_key_violation, is_unique_violation,
    },
};

#[derive(Debug, Clone)]
pub struct AgentRepository {
    database: Database,
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
        let result = match &self.database {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "INSERT INTO agents (id, tenant_id, name, status, version, last_seen_at, created_at)
                     VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5)",
                )
                .bind(agent.id.to_string())
                .bind(agent.tenant_id.to_string())
                .bind(&agent.name)
                .bind(agent.status.as_str())
                .bind(&agent.created_at)
                .execute(pool)
                .await
                .map(|_| ())
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO agents (id, tenant_id, name, status, version, last_seen_at, created_at)
                     VALUES ($1, $2, $3, $4, NULL, NULL, $5)",
                )
                .bind(agent.id.to_string())
                .bind(agent.tenant_id.to_string())
                .bind(&agent.name)
                .bind(agent.status.as_str())
                .bind(&agent.created_at)
                .execute(pool)
                .await
                .map(|_| ())
            }
        };

        match result {
            Ok(_) => Ok(agent),
            Err(err)
                if is_unique_violation(
                    &err,
                    "agents.tenant_id, agents.name",
                    "agents_tenant_id_name_key",
                ) =>
            {
                Err(RepositoryError::DuplicateAgentName)
            }
            Err(err) if is_foreign_key_violation(&err) => Err(RepositoryError::MissingTenant),
            Err(err) => Err(anyhow::Error::new(err)
                .context("failed to insert agent")
                .into()),
        }
    }

    pub async fn list_for_tenant(&self, tenant_id: TenantId) -> RepositoryResult<Vec<Agent>> {
        if !self.tenant_exists(tenant_id).await? {
            return Err(RepositoryError::MissingTenant);
        }

        match &self.database {
            Database::Sqlite(pool) => {
                let rows = sqlx::query(
                    "SELECT id, tenant_id, name, status, created_at
                     FROM agents
                     WHERE tenant_id = ?1
                     ORDER BY created_at ASC, id ASC",
                )
                .bind(tenant_id.to_string())
                .fetch_all(pool)
                .await
                .context("failed to list SQLite agents")?;
                rows.into_iter()
                    .map(|row| {
                        agent_from_parts(
                            row.get("id"),
                            row.get("tenant_id"),
                            row.get("name"),
                            row.get("status"),
                            row.get("created_at"),
                        )
                    })
                    .collect()
            }
            Database::Postgres(pool) => {
                let rows = sqlx::query(
                    "SELECT id, tenant_id, name, status, created_at
                     FROM agents
                     WHERE tenant_id = $1
                     ORDER BY created_at ASC, id ASC",
                )
                .bind(tenant_id.to_string())
                .fetch_all(pool)
                .await
                .context("failed to list PostgreSQL agents")?;
                rows.into_iter()
                    .map(|row| {
                        agent_from_parts(
                            row.get("id"),
                            row.get("tenant_id"),
                            row.get("name"),
                            row.get("status"),
                            row.get("created_at"),
                        )
                    })
                    .collect()
            }
        }
    }

    pub async fn get(&self, agent_id: AgentId) -> RepositoryResult<Option<Agent>> {
        match &self.database {
            Database::Sqlite(pool) => {
                let row = sqlx::query(
                    "SELECT id, tenant_id, name, status, created_at
                     FROM agents
                     WHERE id = ?1",
                )
                .bind(agent_id.to_string())
                .fetch_optional(pool)
                .await
                .context("failed to get SQLite agent")?;
                row.map(|row| {
                    agent_from_parts(
                        row.get("id"),
                        row.get("tenant_id"),
                        row.get("name"),
                        row.get("status"),
                        row.get("created_at"),
                    )
                })
                .transpose()
            }
            Database::Postgres(pool) => {
                let row = sqlx::query(
                    "SELECT id, tenant_id, name, status, created_at
                     FROM agents
                     WHERE id = $1",
                )
                .bind(agent_id.to_string())
                .fetch_optional(pool)
                .await
                .context("failed to get PostgreSQL agent")?;
                row.map(|row| {
                    agent_from_parts(
                        row.get("id"),
                        row.get("tenant_id"),
                        row.get("name"),
                        row.get("status"),
                        row.get("created_at"),
                    )
                })
                .transpose()
            }
        }
    }

    pub async fn update_connection(
        &self,
        agent_id: AgentId,
        status: AgentStatus,
        version: Option<&str>,
        last_seen_at: &str,
    ) -> RepositoryResult<Agent> {
        let rows_affected = match &self.database {
            Database::Sqlite(pool) => sqlx::query(
                "UPDATE agents
                     SET status = ?2, version = COALESCE(?3, version), last_seen_at = ?4
                     WHERE id = ?1",
            )
            .bind(agent_id.to_string())
            .bind(status.as_str())
            .bind(version)
            .bind(last_seen_at)
            .execute(pool)
            .await
            .map(|result| result.rows_affected()),
            Database::Postgres(pool) => sqlx::query(
                "UPDATE agents
                     SET status = $2, version = COALESCE($3, version), last_seen_at = $4
                     WHERE id = $1",
            )
            .bind(agent_id.to_string())
            .bind(status.as_str())
            .bind(version)
            .bind(last_seen_at)
            .execute(pool)
            .await
            .map(|result| result.rows_affected()),
        }
        .context("failed to update agent connection")?;

        if rows_affected == 0 {
            return Err(RepositoryError::MissingAgent);
        }

        self.get(agent_id)
            .await?
            .ok_or(RepositoryError::MissingAgent)
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
        let count = match &self.database {
            Database::Sqlite(pool) => {
                sqlx::query_scalar("SELECT COUNT(*) FROM agents")
                    .fetch_one(pool)
                    .await
            }
            Database::Postgres(pool) => {
                sqlx::query_scalar("SELECT COUNT(*) FROM agents")
                    .fetch_one(pool)
                    .await
            }
        }
        .context("failed to count agents")?;

        Ok(count)
    }

    async fn tenant_exists(&self, tenant_id: TenantId) -> RepositoryResult<bool> {
        let exists = match &self.database {
            Database::Sqlite(pool) => {
                sqlx::query_scalar::<_, i64>("SELECT 1 FROM tenants WHERE id = ?1")
                    .bind(tenant_id.to_string())
                    .fetch_optional(pool)
                    .await
            }
            Database::Postgres(pool) => {
                sqlx::query_scalar::<_, i64>("SELECT 1 FROM tenants WHERE id = $1")
                    .bind(tenant_id.to_string())
                    .fetch_optional(pool)
                    .await
            }
        }
        .context("failed to check tenant existence")?;

        Ok(exists.is_some())
    }
}

fn agent_from_parts(
    id: String,
    tenant_id: String,
    name: String,
    status: String,
    created_at: String,
) -> RepositoryResult<Agent> {
    let status = status
        .parse::<AgentStatus>()
        .map_err(|_| RepositoryError::InvalidPersistedStatus(status.clone()))?;
    Agent::from_parts(
        AgentId::parse(&id).map_err(anyhow::Error::from)?,
        TenantId::parse(&tenant_id).map_err(anyhow::Error::from)?,
        name,
        status,
        created_at,
    )
    .map_err(anyhow::Error::from)
    .context("failed to rehydrate agent")
    .map_err(RepositoryError::from)
}
