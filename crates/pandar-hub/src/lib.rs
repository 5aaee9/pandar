pub mod artifacts;
mod bootstrap;
pub mod cleanup;
pub mod cluster;
pub mod db;
pub mod entities;
pub mod grpc;
pub mod identity;
pub mod jobs;
pub mod metrics;
mod metrics_export;
pub mod printer_events;
pub mod protocol;
pub mod readiness;
pub mod redaction;
pub mod repositories;
mod routes;
pub mod runtime;
pub mod sessions;

use std::{fmt, sync::Arc};

#[cfg(test)]
use anyhow::Context;

use crate::{
    artifacts::{ArtifactStorage, ArtifactStorageConfig, IntoArtifactStorage, JobStorageAlias},
    db::{Database, DatabaseConfig},
    identity::{ExternalAuthConfig, JwtVerifier},
    metrics::{ControlPlaneMetric, MetricsState},
    printer_events::{PrinterEvent, PrinterEventHub},
    repositories::{
        AgentRepository, AuditEventRepository, AuthRepository, CommandRepository, JobRepository,
        MaterialRepository, PrinterEventTicketRepository, PrinterRepository, TenantRepository,
    },
    sessions::SessionRegistry,
};

#[derive(Clone)]
pub struct AppState {
    database: Database,
    tenants: TenantRepository,
    auth: AuthRepository,
    audit_events: AuditEventRepository,
    agents: AgentRepository,
    printers: PrinterRepository,
    commands: CommandRepository,
    jobs: JobRepository,
    materials: MaterialRepository,
    printer_event_tickets: PrinterEventTicketRepository,
    artifact_storage: Arc<dyn ArtifactStorage>,
    external_auth: Option<JwtVerifier>,
    bootstrap_token: Option<String>,
    printer_events: PrinterEventHub,
    sessions: SessionRegistry,
    metrics: MetricsState,
    control_plane: cluster::ControlPlane,
    #[cfg(test)]
    database_backend_override: Option<db::DatabaseBackend>,
}

impl fmt::Debug for AppState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppState")
            .field("artifact_storage_backend", &self.artifact_storage.backend())
            .finish()
    }
}

impl AppState {
    pub async fn connect(database_url: impl Into<String>) -> anyhow::Result<Self> {
        let artifact_storage = ArtifactStorageConfig::from_env()?.build().await?;
        Self::connect_with_config(database_url, artifact_storage).await
    }

    pub async fn connect_with_config(
        database_url: impl Into<String>,
        artifact_storage: impl IntoArtifactStorage,
    ) -> anyhow::Result<Self> {
        let external_auth = ExternalAuthConfig::from_env()?.map(JwtVerifier::remote);
        Self::connect_with_auth_config(database_url, artifact_storage, external_auth).await
    }

    pub async fn connect_with_auth_config(
        database_url: impl Into<String>,
        artifact_storage: impl IntoArtifactStorage,
        external_auth: Option<JwtVerifier>,
    ) -> anyhow::Result<Self> {
        let control_plane = std::env::var("PANDAR_CONTROL_PLANE").ok();
        let nats_url = std::env::var("PANDAR_NATS_URL").ok();
        let nats_subject = std::env::var("PANDAR_NATS_SUBJECT").ok();
        Self::connect_with_config_values(
            database_url,
            artifact_storage,
            external_auth,
            control_plane.as_deref(),
            nats_url.as_deref(),
            nats_subject.as_deref(),
        )
        .await
    }

    pub async fn connect_with_config_values(
        database_url: impl Into<String>,
        artifact_storage: impl IntoArtifactStorage,
        external_auth: Option<JwtVerifier>,
        control_plane: Option<&str>,
        nats_url: Option<&str>,
        nats_subject: Option<&str>,
    ) -> anyhow::Result<Self> {
        let database_url = database_url.into();
        let config = DatabaseConfig::from_url(database_url)?;
        let control_plane_config = cluster::ControlPlaneConfig::from_values(
            config.backend(),
            control_plane,
            nats_url,
            nats_subject,
        )?;
        let control_plane = cluster::ControlPlane::from_config(control_plane_config).await?;
        let database = Database::connect(&config).await?;
        database.migrate().await?;

        let bootstrap_token = std::env::var("PANDAR_BOOTSTRAP_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty());

        Ok(
            Self::from_database_with_control_plane(database, artifact_storage, control_plane)
                .with_external_auth_option(external_auth)
                .with_bootstrap_token_option(bootstrap_token),
        )
    }

    pub fn from_database(database: Database, artifact_storage: impl IntoArtifactStorage) -> Self {
        Self::from_database_with_control_plane(
            database,
            artifact_storage,
            cluster::ControlPlane::in_process(),
        )
    }

    pub fn from_database_with_control_plane(
        database: Database,
        artifact_storage: impl IntoArtifactStorage,
        control_plane: cluster::ControlPlane,
    ) -> Self {
        let metrics = MetricsState::new();
        Self {
            database: database.clone(),
            tenants: TenantRepository::new(database.clone()),
            auth: AuthRepository::new(database.clone()),
            audit_events: AuditEventRepository::new(database.clone()),
            agents: AgentRepository::new(database.clone()),
            printers: PrinterRepository::new(database.clone()),
            commands: CommandRepository::new(database.clone()),
            jobs: JobRepository::new(database.clone()),
            materials: MaterialRepository::new(database.clone()),
            printer_event_tickets: PrinterEventTicketRepository::new(database),
            artifact_storage: artifact_storage.into_artifact_storage(),
            external_auth: None,
            bootstrap_token: None,
            printer_events: PrinterEventHub::with_metrics(metrics.clone()),
            sessions: SessionRegistry::new(),
            metrics,
            control_plane,
            #[cfg(test)]
            database_backend_override: None,
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

    fn with_bootstrap_token_option(mut self, token: Option<String>) -> Self {
        self.bootstrap_token = token;
        self
    }

    #[cfg(test)]
    pub fn with_bootstrap_token(self, token: impl Into<String>) -> Self {
        self.with_bootstrap_token_option(Some(token.into()))
    }

    #[cfg(test)]
    pub async fn sqlite_for_tests() -> anyhow::Result<Self> {
        let temp_dir = tempfile::tempdir()
            .context("failed to create temporary job spool directory")?
            .keep();
        let artifact_storage = artifacts::FilesystemArtifactStorage::new(
            temp_dir,
            artifacts::DEFAULT_MAX_ARTIFACT_BYTES,
        )?;
        Self::connect_with_config_values(
            "sqlite::memory:",
            artifact_storage,
            None,
            None,
            None,
            None,
        )
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

    pub fn materials(&self) -> &MaterialRepository {
        &self.materials
    }

    pub fn printer_event_tickets(&self) -> &PrinterEventTicketRepository {
        &self.printer_event_tickets
    }

    pub fn artifact_storage(&self) -> &dyn ArtifactStorage {
        &*self.artifact_storage
    }

    pub fn job_storage(&self) -> JobStorageAlias<'_> {
        JobStorageAlias::new(self.artifact_storage())
    }

    pub fn external_auth(&self) -> Option<&JwtVerifier> {
        self.external_auth.as_ref()
    }

    pub fn bootstrap_token(&self) -> Option<&str> {
        self.bootstrap_token.as_deref()
    }

    pub fn printer_events(&self) -> &PrinterEventHub {
        &self.printer_events
    }

    pub fn sessions(&self) -> &SessionRegistry {
        &self.sessions
    }

    pub fn metrics(&self) -> &MetricsState {
        &self.metrics
    }

    pub fn control_plane(&self) -> &cluster::ControlPlane {
        &self.control_plane
    }

    pub(crate) fn database_backend(&self) -> db::DatabaseBackend {
        #[cfg(test)]
        if let Some(backend) = self.database_backend_override {
            return backend;
        }
        self.database.backend()
    }

    pub async fn wake_agent(
        &self,
        tenant_id: pandar_core::TenantId,
        agent_id: pandar_core::AgentId,
    ) {
        if let Err(err) = self
            .control_plane
            .publish(cluster::HubControlMessage::AgentWake {
                tenant_id: tenant_id.to_string(),
                agent_id: agent_id.to_string(),
            })
            .await
        {
            self.metrics
                .record_control_plane(ControlPlaneMetric::PublishFailed);
            tracing::error!(error = %format!("{err:#}"), "failed to publish agent wake control message");
        } else {
            self.metrics
                .record_control_plane(ControlPlaneMetric::PublishOk);
        }
    }

    pub async fn close_agent(
        &self,
        tenant_id: pandar_core::TenantId,
        agent_id: pandar_core::AgentId,
    ) {
        self.sessions.close_local_agent(tenant_id, agent_id).await;
        if let Err(err) = self
            .control_plane
            .publish(cluster::HubControlMessage::AgentClose {
                tenant_id: tenant_id.to_string(),
                agent_id: agent_id.to_string(),
            })
            .await
        {
            self.metrics
                .record_control_plane(ControlPlaneMetric::PublishFailed);
            tracing::error!(error = %format!("{err:#}"), "failed to publish agent close control message");
        } else {
            self.metrics
                .record_control_plane(ControlPlaneMetric::PublishOk);
        }
    }

    pub async fn publish_printer_event(
        &self,
        tenant_id: pandar_core::TenantId,
        event: PrinterEvent,
    ) {
        if let Err(err) = self
            .control_plane
            .publish(cluster::HubControlMessage::PrinterEvent {
                tenant_id: tenant_id.to_string(),
                event,
            })
            .await
        {
            self.metrics
                .record_control_plane(ControlPlaneMetric::PublishFailed);
            tracing::error!(error = %format!("{err:#}"), "failed to publish printer event control message");
        } else {
            self.metrics
                .record_control_plane(ControlPlaneMetric::PublishOk);
        }
    }

    pub(crate) fn database(&self) -> &Database {
        &self.database
    }

    #[cfg(test)]
    pub(crate) fn sibling_for_tests(&self) -> Self {
        Self::from_database_with_control_plane(
            self.database.clone(),
            self.artifact_storage.clone(),
            self.control_plane.clone(),
        )
        .with_external_auth_option(self.external_auth.clone())
        .with_bootstrap_token_option(self.bootstrap_token.clone())
    }

    #[cfg(test)]
    pub(crate) fn with_control_plane_for_tests(
        mut self,
        control_plane: cluster::ControlPlane,
    ) -> Self {
        self.control_plane = control_plane;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_database_backend_for_tests(mut self, backend: db::DatabaseBackend) -> Self {
        self.database_backend_override = Some(backend);
        self
    }
}

pub use bootstrap::run_from_env;
pub use routes::router;
