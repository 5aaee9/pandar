use anyhow::Context;
use pandar_core::{Agent, AgentId, AgentStatus, TenantId};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ConnectionTrait, DatabaseTransaction, EntityTrait,
    IntoActiveModel, QuerySelect, SqliteTransactionMode, TransactionOptions, TransactionTrait,
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use super::{AgentRepository, agent_from_model};
use crate::{
    db::Database,
    entities::agents,
    repositories::{RepositoryError, RepositoryResult},
};

impl AgentRepository {
    pub(crate) async fn begin_current_session_fence(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        session_id: &str,
    ) -> RepositoryResult<DatabaseTransaction> {
        begin_current_agent_transaction(&self.database, tenant_id, agent_id, session_id).await
    }

    #[cfg(test)]
    pub(crate) async fn update_connection(
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

        let mut active = agent.into_active_model();
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

    #[cfg(test)]
    pub(crate) async fn mark_offline(
        &self,
        agent_id: AgentId,
        last_seen_at: &str,
    ) -> RepositoryResult<Agent> {
        self.update_connection(agent_id, AgentStatus::Offline, None, last_seen_at)
            .await
    }

    pub async fn claim_online_session(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        session_id: &str,
        version: &str,
        last_seen_at: &str,
    ) -> RepositoryResult<Agent> {
        let tx = begin_agent_transaction(&self.database).await?;
        let agent = locked_agent(&tx, agent_id)
            .await?
            .ok_or(RepositoryError::MissingAgent)?;
        if agent.tenant_id != tenant_id.to_string() {
            return Err(RepositoryError::MissingAgent);
        }

        let mut active = agent.into_active_model();
        active.status = Set(AgentStatus::Online.as_str().to_owned());
        active.version = Set(Some(version.to_owned()));
        active.last_seen_at = Set(Some(last_seen_at.to_owned()));
        active.current_session_id = Set(Some(session_id.to_owned()));
        let agent = active
            .update(&tx)
            .await
            .context("failed to claim online agent session")
            .map_err(RepositoryError::from)
            .and_then(agent_from_model)?;
        tx.commit()
            .await
            .context("failed to commit online agent session claim")?;
        Ok(agent)
    }

    pub async fn heartbeat_if_current(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        session_id: &str,
        last_seen_at: &str,
    ) -> RepositoryResult<Agent> {
        let tx = begin_current_agent_transaction(&self.database, tenant_id, agent_id, session_id)
            .await?;
        let agent = agents::Entity::find_by_id(agent_id.to_string())
            .one(&tx)
            .await
            .context("failed to reload current agent for heartbeat")?
            .ok_or(RepositoryError::MissingAgent)?;
        let mut active = agent.into_active_model();
        active.status = Set(AgentStatus::Online.as_str().to_owned());
        active.last_seen_at = Set(Some(last_seen_at.to_owned()));
        let agent = active
            .update(&tx)
            .await
            .context("failed to update current agent heartbeat")
            .map_err(RepositoryError::from)
            .and_then(agent_from_model)?;
        tx.commit()
            .await
            .context("failed to commit current agent heartbeat")?;
        Ok(agent)
    }

    pub async fn mark_offline_if_current(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        session_id: &str,
        last_seen_at: &str,
    ) -> RepositoryResult<Option<Agent>> {
        let tx = begin_agent_transaction(&self.database).await?;
        let agent = locked_agent(&tx, agent_id)
            .await?
            .ok_or(RepositoryError::MissingAgent)?;
        if agent.tenant_id != tenant_id.to_string()
            || agent.current_session_id.as_deref() != Some(session_id)
        {
            tx.commit()
                .await
                .context("failed to commit stale agent offline comparison")?;
            return Ok(None);
        }

        let mut active = agent.into_active_model();
        active.status = Set(AgentStatus::Offline.as_str().to_owned());
        active.last_seen_at = Set(Some(last_seen_at.to_owned()));
        active.current_session_id = Set(None);
        let agent = active
            .update(&tx)
            .await
            .context("failed to mark current agent session offline")
            .map_err(RepositoryError::from)
            .and_then(agent_from_model)?;
        tx.commit()
            .await
            .context("failed to commit current agent offline transition")?;
        Ok(Some(agent))
    }
}

pub async fn begin_current_agent_transaction(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
    session_id: &str,
) -> RepositoryResult<DatabaseTransaction> {
    let tx = begin_agent_transaction(database).await?;
    let agent = locked_agent(&tx, agent_id)
        .await?
        .ok_or(RepositoryError::MissingAgent)?;
    if agent.tenant_id != tenant_id.to_string()
        || agent.current_session_id.as_deref() != Some(session_id)
    {
        return Err(RepositoryError::AgentSessionNotCurrent);
    }
    #[cfg(test)]
    current_transaction_pause::wait(session_id, &tx).await;
    Ok(tx)
}

pub(crate) async fn begin_stale_firmware_cleanup_transaction(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
    owner_session_id: &str,
    owner_instance_id: uuid::Uuid,
    sweeper_instance_id: uuid::Uuid,
    cutoff: OffsetDateTime,
) -> RepositoryResult<Option<DatabaseTransaction>> {
    let tx = begin_agent_transaction(database).await?;
    let agent = locked_agent(&tx, agent_id).await?;
    let has_fresh_owner = match agent {
        Some(agent)
            if agent.tenant_id == tenant_id.to_string()
                && agent.current_session_id.as_deref() == Some(owner_session_id) =>
        {
            agent
                .last_seen_at
                .as_deref()
                .map(|last_seen_at| {
                    OffsetDateTime::parse(last_seen_at, &Rfc3339)
                        .context("failed to parse agent heartbeat during firmware cleanup")
                })
                .transpose()?
                .is_some_and(|last_seen_at| last_seen_at > cutoff)
        }
        _ => false,
    };
    if has_fresh_owner && owner_instance_id != sweeper_instance_id {
        tx.commit()
            .await
            .context("failed to commit fresh firmware owner cleanup comparison")?;
        return Ok(None);
    }
    Ok(Some(tx))
}

async fn begin_agent_transaction(database: &Database) -> RepositoryResult<DatabaseTransaction> {
    database
        .sea_orm_connection()
        .begin_with_options(TransactionOptions {
            sqlite_transaction_mode: matches!(database, Database::Sqlite(_))
                .then_some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await
        .context("failed to begin agent transaction")
        .map_err(Into::into)
}

async fn locked_agent<C>(
    connection: &C,
    agent_id: AgentId,
) -> RepositoryResult<Option<agents::Model>>
where
    C: ConnectionTrait,
{
    let query = agents::Entity::find_by_id(agent_id.to_string());
    match connection.get_database_backend() {
        sea_orm::DatabaseBackend::Postgres => query.lock_exclusive().one(connection).await,
        _ => query.one(connection).await,
    }
    .context("failed to lock agent connection row")
    .map_err(Into::into)
}

#[cfg(test)]
pub(crate) mod current_transaction_pause {
    use std::{
        collections::HashMap,
        sync::{Mutex, OnceLock},
        time::Duration,
    };

    use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseTransaction, Statement};
    use tokio::sync::oneshot;

    const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

    struct PausePoint {
        reached: oneshot::Sender<Option<i32>>,
        resume: oneshot::Receiver<()>,
    }

    pub(crate) struct TransactionPause {
        reached: oneshot::Receiver<Option<i32>>,
        resume: Option<oneshot::Sender<()>>,
    }

    pub(crate) fn install(session_id: &str) -> TransactionPause {
        let (reached_sender, reached_receiver) = oneshot::channel();
        let (resume_sender, resume_receiver) = oneshot::channel();
        let previous = pauses()
            .lock()
            .expect("current transaction pause mutex should not be poisoned")
            .insert(
                session_id.to_owned(),
                PausePoint {
                    reached: reached_sender,
                    resume: resume_receiver,
                },
            );
        assert!(
            previous.is_none(),
            "current transaction pause already installed"
        );
        TransactionPause {
            reached: reached_receiver,
            resume: Some(resume_sender),
        }
    }

    impl TransactionPause {
        pub(crate) async fn wait_until_reached(&mut self) -> Option<i32> {
            tokio::time::timeout(WAIT_TIMEOUT, &mut self.reached)
                .await
                .expect("timed out waiting for current transaction pause")
                .expect("current transaction pause was dropped before being reached")
        }

        pub(crate) fn resume(mut self) {
            let _ = self
                .resume
                .take()
                .expect("current transaction resume sender must be present")
                .send(());
        }
    }

    pub(crate) async fn wait(session_id: &str, transaction: &DatabaseTransaction) {
        let pause = pauses()
            .lock()
            .expect("current transaction pause mutex should not be poisoned")
            .remove(session_id);
        if let Some(pause) = pause {
            let backend_pid = match transaction.get_database_backend() {
                DatabaseBackend::Postgres => transaction
                    .query_one_raw(Statement::from_string(
                        DatabaseBackend::Postgres,
                        "SELECT pg_backend_pid()".to_owned(),
                    ))
                    .await
                    .expect("failed to query paused PostgreSQL backend PID")
                    .expect("PostgreSQL backend PID query returned no row")
                    .try_get_by_index(0)
                    .map(Some)
                    .expect("failed to decode paused PostgreSQL backend PID"),
                _ => None,
            };
            let _ = pause.reached.send(backend_pid);
            let _ = pause.resume.await;
        }
    }

    fn pauses() -> &'static Mutex<HashMap<String, PausePoint>> {
        static PAUSES: OnceLock<Mutex<HashMap<String, PausePoint>>> = OnceLock::new();
        PAUSES.get_or_init(|| Mutex::new(HashMap::new()))
    }
}
