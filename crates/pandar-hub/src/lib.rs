pub mod db;
pub mod grpc;
pub mod protocol;
pub mod repositories;
mod routes;
pub mod runtime;
pub mod sessions;

use anyhow::Context;

use crate::{
    db::{Database, DatabaseConfig},
    repositories::{AgentRepository, CommandRepository, PrinterRepository, TenantRepository},
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
    sessions: SessionRegistry,
}

impl AppState {
    pub async fn connect(database_url: impl Into<String>) -> anyhow::Result<Self> {
        let database_url = database_url.into();
        let config = DatabaseConfig::from_url(database_url)?;
        let database = Database::connect(&config).await?;
        database.migrate().await?;

        Ok(Self::from_database(database))
    }

    pub fn from_database(database: Database) -> Self {
        Self {
            #[cfg(test)]
            database: database.clone(),
            tenants: TenantRepository::new(database.clone()),
            agents: AgentRepository::new(database.clone()),
            printers: PrinterRepository::new(database.clone()),
            commands: CommandRepository::new(database),
            sessions: SessionRegistry::new(),
        }
    }

    pub async fn sqlite_for_tests() -> anyhow::Result<Self> {
        Self::connect("sqlite::memory:")
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

    pub fn sessions(&self) -> &SessionRegistry {
        &self.sessions
    }

    #[cfg(test)]
    pub(crate) fn database(&self) -> &Database {
        &self.database
    }
}

pub use routes::router;
