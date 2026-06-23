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

use anyhow::Context;
use tokio::net::TcpListener;
use tonic::transport::Server;

use crate::{
    db::{Database, DatabaseConfig},
    grpc::AgentControlService,
    identity::{ExternalAuthConfig, JwtVerifier},
    jobs::JobStorageConfig,
    metrics::MetricsState,
    printer_events::{PrinterEvent, PrinterEventHub},
    protocol::agent::v1::agent_control_server::AgentControlServer,
    repositories::{
        AgentRepository, AuditEventRepository, AuthRepository, CommandRepository, JobRepository,
        MaterialRepository, PrinterEventTicketRepository, PrinterRepository, TenantRepository,
    },
    sessions::SessionRegistry,
};

#[derive(Debug, Clone)]
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
    job_storage: JobStorageConfig,
    external_auth: Option<JwtVerifier>,
    bootstrap_token: Option<String>,
    printer_events: PrinterEventHub,
    sessions: SessionRegistry,
    metrics: MetricsState,
    control_plane: cluster::ControlPlane,
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
        let control_plane = std::env::var("PANDAR_CONTROL_PLANE").ok();
        let nats_url = std::env::var("PANDAR_NATS_URL").ok();
        let nats_subject = std::env::var("PANDAR_NATS_SUBJECT").ok();
        Self::connect_with_config_values(
            database_url,
            job_storage,
            external_auth,
            control_plane.as_deref(),
            nats_url.as_deref(),
            nats_subject.as_deref(),
        )
        .await
    }

    pub async fn connect_with_config_values(
        database_url: impl Into<String>,
        job_storage: JobStorageConfig,
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
            Self::from_database_with_control_plane(database, job_storage, control_plane)
                .with_external_auth_option(external_auth)
                .with_bootstrap_token_option(bootstrap_token),
        )
    }

    pub fn from_database(database: Database, job_storage: JobStorageConfig) -> Self {
        Self::from_database_with_control_plane(
            database,
            job_storage,
            cluster::ControlPlane::in_process(),
        )
    }

    pub fn from_database_with_control_plane(
        database: Database,
        job_storage: JobStorageConfig,
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
            job_storage,
            external_auth: None,
            bootstrap_token: None,
            printer_events: PrinterEventHub::with_metrics(metrics.clone()),
            sessions: SessionRegistry::new(),
            metrics,
            control_plane,
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
        let job_storage = JobStorageConfig::new(temp_dir, jobs::DEFAULT_MAX_ARTIFACT_BYTES)?;
        Self::connect_with_config_values("sqlite::memory:", job_storage, None, None, None, None)
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

    pub fn job_storage(&self) -> &JobStorageConfig {
        &self.job_storage
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
            tracing::error!(error = %format!("{err:#}"), "failed to publish agent wake control message");
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
            tracing::error!(error = %format!("{err:#}"), "failed to publish agent close control message");
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
            tracing::error!(error = %format!("{err:#}"), "failed to publish printer event control message");
        }
    }

    pub(crate) fn database(&self) -> &Database {
        &self.database
    }

    #[cfg(test)]
    pub(crate) fn sibling_for_tests(&self) -> Self {
        Self::from_database_with_control_plane(
            self.database.clone(),
            self.job_storage.clone(),
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
}

pub use routes::router;

pub async fn run_from_env() -> anyhow::Result<()> {
    let bind_addr = std::env::var("PANDAR_HUB_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());
    let grpc_bind_addr =
        std::env::var("PANDAR_HUB_GRPC_BIND").unwrap_or_else(|_| "0.0.0.0:50051".to_owned());
    let database_url =
        std::env::var("PANDAR_DATABASE_URL").unwrap_or_else(|_| "sqlite://pandar.db".to_owned());
    let state = AppState::connect(database_url)
        .await
        .context("failed to initialize pandar-hub application state")?;
    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("failed to bind pandar-hub to {bind_addr}"))?;
    let grpc_listener = TcpListener::bind(&grpc_bind_addr)
        .await
        .with_context(|| format!("failed to bind pandar-hub gRPC to {grpc_bind_addr}"))?;

    tracing::info!(%bind_addr, "pandar-hub listening");
    tracing::info!(%grpc_bind_addr, "pandar-hub gRPC listening");
    let _session_expiry = runtime::spawn_session_expiry(state.clone());
    let (_control_plane, control_plane_ready) = runtime::spawn_control_plane_ready(state.clone());
    control_plane_ready
        .await
        .context("control plane subscriber stopped before reporting readiness")?
        .context("failed to start control plane subscriber")?;
    let http = axum::serve(listener, router(state.clone()));
    let grpc = Server::builder()
        .add_service(AgentControlServer::new(AgentControlService::new(state)))
        .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(
            grpc_listener,
        ));

    tokio::try_join!(
        async { http.await.context("pandar-hub HTTP server exited") },
        async { grpc.await.context("pandar-hub gRPC server exited") },
    )?;

    Ok(())
}
