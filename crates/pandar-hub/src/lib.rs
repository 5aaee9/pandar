pub mod db;
pub mod grpc;
pub mod jobs;
pub mod printer_events;
pub mod protocol;
pub mod repositories;
mod routes;
pub mod runtime;
pub mod sessions;

#[cfg(test)]
use anyhow::Context;

use crate::{
    db::{Database, DatabaseConfig},
    jobs::JobStorageConfig,
    printer_events::PrinterEventHub,
    repositories::{
        AgentRepository, CommandRepository, JobRepository, PrinterRepository, TenantRepository,
    },
    sessions::SessionRegistry,
};

#[derive(Debug, Clone)]
pub struct AppState {
    #[cfg(test)]
    database: Database,
    tenants: TenantRepository,
    agents: AgentRepository,
    printers: PrinterRepository,
    commands: CommandRepository,
    jobs: JobRepository,
    job_storage: JobStorageConfig,
    printer_events: PrinterEventHub,
    sessions: SessionRegistry,
}

impl AppState {
    pub async fn connect(database_url: impl Into<String>) -> anyhow::Result<Self> {
        let job_storage = JobStorageConfig::from_env()?;
        Self::connect_with_config(database_url, job_storage).await
    }

    pub async fn connect_with_config(
        database_url: impl Into<String>,
        job_storage: JobStorageConfig,
    ) -> anyhow::Result<Self> {
        let database_url = database_url.into();
        let config = DatabaseConfig::from_url(database_url)?;
        let database = Database::connect(&config).await?;
        database.migrate().await?;

        Ok(Self::from_database(database, job_storage))
    }

    pub fn from_database(database: Database, job_storage: JobStorageConfig) -> Self {
        Self {
            #[cfg(test)]
            database: database.clone(),
            tenants: TenantRepository::new(database.clone()),
            agents: AgentRepository::new(database.clone()),
            printers: PrinterRepository::new(database.clone()),
            commands: CommandRepository::new(database.clone()),
            jobs: JobRepository::new(database),
            job_storage,
            printer_events: PrinterEventHub::new(),
            sessions: SessionRegistry::new(),
        }
    }

    #[cfg(test)]
    pub async fn sqlite_for_tests() -> anyhow::Result<Self> {
        let temp_dir = tempfile::tempdir()
            .context("failed to create temporary job spool directory")?
            .keep();
        let job_storage = JobStorageConfig::new(temp_dir, jobs::DEFAULT_MAX_ARTIFACT_BYTES)?;
        Self::connect_with_config("sqlite::memory:", job_storage)
            .await
            .context("failed to create SQLite test app state")
    }

    pub fn tenants(&self) -> &TenantRepository {
        &self.tenants
    }

    pub fn agents(&self) -> &AgentRepository {
        &self.agents
    }

    pub fn printers(&self) -> &PrinterRepository {
        &self.printers
    }

    pub fn commands(&self) -> &CommandRepository {
        &self.commands
    }

    pub fn jobs(&self) -> &JobRepository {
        &self.jobs
    }

    pub fn job_storage(&self) -> &JobStorageConfig {
        &self.job_storage
    }

    pub fn printer_events(&self) -> &PrinterEventHub {
        &self.printer_events
    }

    pub fn sessions(&self) -> &SessionRegistry {
        &self.sessions
    }

    #[cfg(test)]
    pub(crate) fn database(&self) -> &Database {
        &self.database
    }
}

pub use routes::router;
