pub mod db;
pub mod entities;
pub mod grpc;
pub mod identity;
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
    identity::{ExternalAuthConfig, JwtVerifier},
    jobs::JobStorageConfig,
    printer_events::PrinterEventHub,
    repositories::{
        AgentRepository, AuditEventRepository, AuthRepository, CommandRepository, JobRepository,
        PrinterRepository, TenantRepository,
    },
    sessions::SessionRegistry,
};

#[derive(Debug, Clone)]
pub struct AppState {
    #[cfg(test)]
    database: Database,
    tenants: TenantRepository,
    auth: AuthRepository,
    audit_events: AuditEventRepository,
    agents: AgentRepository,
    printers: PrinterRepository,
    commands: CommandRepository,
    jobs: JobRepository,
    job_storage: JobStorageConfig,
    external_auth: Option<JwtVerifier>,
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
        let external_auth = ExternalAuthConfig::from_env()?.map(JwtVerifier::remote);
        Self::connect_with_auth_config(database_url, job_storage, external_auth).await
    }

    pub async fn connect_with_auth_config(
        database_url: impl Into<String>,
        job_storage: JobStorageConfig,
        external_auth: Option<JwtVerifier>,
    ) -> anyhow::Result<Self> {
        let database_url = database_url.into();
        let config = DatabaseConfig::from_url(database_url)?;
        let database = Database::connect(&config).await?;
        database.migrate().await?;

        Ok(Self::from_database(database, job_storage).with_external_auth_option(external_auth))
    }

    pub fn from_database(database: Database, job_storage: JobStorageConfig) -> Self {
        Self {
            #[cfg(test)]
            database: database.clone(),
            tenants: TenantRepository::new(database.clone()),
            auth: AuthRepository::new(database.clone()),
            audit_events: AuditEventRepository::new(database.clone()),
            agents: AgentRepository::new(database.clone()),
            printers: PrinterRepository::new(database.clone()),
            commands: CommandRepository::new(database.clone()),
            jobs: JobRepository::new(database),
            job_storage,
            external_auth: None,
            printer_events: PrinterEventHub::new(),
            sessions: SessionRegistry::new(),
        }
    }

    fn with_external_auth_option(mut self, verifier: Option<JwtVerifier>) -> Self {
        self.external_auth = verifier;
        self
    }

    #[cfg(test)]
    pub fn with_external_auth(self, verifier: JwtVerifier) -> Self {
        self.with_external_auth_option(Some(verifier))
    }

    #[cfg(test)]
    pub async fn sqlite_for_tests() -> anyhow::Result<Self> {
        let temp_dir = tempfile::tempdir()
            .context("failed to create temporary job spool directory")?
            .keep();
        let job_storage = JobStorageConfig::new(temp_dir, jobs::DEFAULT_MAX_ARTIFACT_BYTES)?;
        Self::connect_with_auth_config("sqlite::memory:", job_storage, None)
            .await
            .context("failed to create SQLite test app state")
    }

    pub fn tenants(&self) -> &TenantRepository {
        &self.tenants
    }

    pub fn auth(&self) -> &AuthRepository {
        &self.auth
    }

    pub fn audit_events(&self) -> &AuditEventRepository {
        &self.audit_events
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

    pub fn external_auth(&self) -> Option<&JwtVerifier> {
        self.external_auth.as_ref()
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
