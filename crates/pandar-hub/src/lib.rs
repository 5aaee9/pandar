mod app_state_accessors;
pub mod artifacts;
mod bootstrap;
pub mod camera_sessions;
pub mod cleanup;
pub mod cluster;
mod config;
pub mod db;
pub mod entities;
pub mod firmware_control;
pub mod grpc;
mod grpc_connection_limit;
pub mod identity;
mod job_projection;
pub mod jobs;
pub(crate) mod material_mapping;
pub mod metrics;
mod metrics_export;
pub mod printer_events;
mod printer_projection;
mod printer_secrets;
pub mod readiness;
pub mod redaction;
pub mod repositories;
mod routes;
pub mod runtime;
pub mod sessions;
#[cfg(test)]
mod test_support;

use std::{fmt, sync::Arc};

use crate::{
    artifacts::{ArtifactStorage, ArtifactStorageConfig, IntoArtifactStorage, JobStorageAlias},
    camera_sessions::CameraSessionRegistry,
    config::{
        camera_max_streams_per_tenant_from_env, no_auth_from_env,
        tenant_self_create_allowed_from_env,
    },
    db::{Database, DatabaseConfig},
    identity::{ExternalAuthConfig, JwtVerifier},
    metrics::{ControlPlaneMetric, MetricsState},
    printer_events::{PrinterEvent, PrinterEventHub},
    printer_secrets::{
        PrinterAccessCodeCipher, configured_printer_access_code_cipher,
        migrate_printer_access_codes,
    },
    repositories::{
        AgentRepository, AuditEventRepository, AuthRepository, CommandRepository, JobRepository,
        MaterialRepository, PersonalPresetRepository, PrinterEventTicketRepository,
        PrinterRepository, TenantRepository,
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
    personal_presets: PersonalPresetRepository,
    printer_event_tickets: PrinterEventTicketRepository,
    artifact_storage: Arc<dyn ArtifactStorage>,
    external_auth: Option<JwtVerifier>,
    no_auth: bool,
    tenant_self_create_allowed: bool,
    bootstrap_token: Option<String>,
    printer_events: PrinterEventHub,
    sessions: SessionRegistry,
    camera_sessions: CameraSessionRegistry,
    metrics: MetricsState,
    control_plane: cluster::ControlPlane,
    instance_id: uuid::Uuid,
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
        let printer_access_code_cipher = configured_printer_access_code_cipher()?;
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
        migrate_printer_access_codes(&database, &printer_access_code_cipher).await?;

        let bootstrap_token = std::env::var("PANDAR_BOOTSTRAP_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let tenant_self_create_allowed = tenant_self_create_allowed_from_env()?;
        let no_auth = no_auth_from_env()?;
        let camera_max_streams_per_tenant = camera_max_streams_per_tenant_from_env()?;

        Ok(Self::from_database_with_control_plane_and_cipher(
            database,
            artifact_storage,
            control_plane,
            printer_access_code_cipher,
        )
        .with_external_auth_option(external_auth)
        .with_no_auth(no_auth)
        .with_tenant_self_create_allowed(tenant_self_create_allowed)
        .with_camera_max_streams_per_tenant(camera_max_streams_per_tenant)
        .with_bootstrap_token_option(bootstrap_token))
    }

    pub async fn from_database(
        database: Database,
        artifact_storage: impl IntoArtifactStorage,
    ) -> anyhow::Result<Self> {
        Self::from_database_with_control_plane(
            database,
            artifact_storage,
            cluster::ControlPlane::in_process(),
        )
        .await
    }

    pub async fn from_database_with_control_plane(
        database: Database,
        artifact_storage: impl IntoArtifactStorage,
        control_plane: cluster::ControlPlane,
    ) -> anyhow::Result<Self> {
        let printer_access_code_cipher = configured_printer_access_code_cipher()?;
        migrate_printer_access_codes(&database, &printer_access_code_cipher).await?;
        Ok(Self::from_database_with_control_plane_and_cipher(
            database,
            artifact_storage,
            control_plane,
            printer_access_code_cipher,
        ))
    }

    fn from_database_with_control_plane_and_cipher(
        database: Database,
        artifact_storage: impl IntoArtifactStorage,
        control_plane: cluster::ControlPlane,
        printer_access_code_cipher: PrinterAccessCodeCipher,
    ) -> Self {
        let metrics = MetricsState::new();
        Self {
            database: database.clone(),
            tenants: TenantRepository::new(database.clone()),
            auth: AuthRepository::new(database.clone()),
            audit_events: AuditEventRepository::new(database.clone()),
            agents: AgentRepository::new(database.clone()),
            printers: PrinterRepository::new_with_cipher(
                database.clone(),
                printer_access_code_cipher.clone(),
            ),
            commands: CommandRepository::new(database.clone()),
            jobs: JobRepository::new_with_cipher(database.clone(), printer_access_code_cipher),
            materials: MaterialRepository::new(database.clone()),
            personal_presets: PersonalPresetRepository::new(database.clone()),
            printer_event_tickets: PrinterEventTicketRepository::new(database),
            artifact_storage: artifact_storage.into_artifact_storage(),
            external_auth: None,
            no_auth: false,
            tenant_self_create_allowed: true,
            bootstrap_token: None,
            printer_events: PrinterEventHub::with_metrics(metrics.clone()),
            sessions: SessionRegistry::new(),
            camera_sessions: CameraSessionRegistry::new(),
            metrics,
            control_plane,
            instance_id: uuid::Uuid::new_v4(),
            #[cfg(test)]
            database_backend_override: None,
        }
    }

    fn with_external_auth_option(mut self, verifier: Option<JwtVerifier>) -> Self {
        self.external_auth = verifier;
        self
    }

    fn with_bootstrap_token_option(mut self, token: Option<String>) -> Self {
        self.bootstrap_token = token;
        self
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

    pub fn no_auth_enabled(&self) -> bool {
        self.no_auth
    }

    fn with_no_auth(mut self, enabled: bool) -> Self {
        self.no_auth = enabled;
        self
    }

    pub fn tenant_self_create_allowed(&self) -> bool {
        self.tenant_self_create_allowed
    }

    fn with_tenant_self_create_allowed(mut self, allowed: bool) -> Self {
        self.tenant_self_create_allowed = allowed;
        self
    }

    fn with_camera_max_streams_per_tenant(mut self, max_streams_per_tenant: usize) -> Self {
        self.camera_sessions =
            CameraSessionRegistry::with_max_streams_per_tenant(max_streams_per_tenant);
        self
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

    pub fn camera_sessions(&self) -> &CameraSessionRegistry {
        &self.camera_sessions
    }

    pub fn metrics(&self) -> &MetricsState {
        &self.metrics
    }

    pub fn control_plane(&self) -> &cluster::ControlPlane {
        &self.control_plane
    }

    pub(crate) fn instance_id(&self) -> uuid::Uuid {
        self.instance_id
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
            self.printer_events.invalidate_epoch(tenant_id);
            tracing::error!(error = %format!("{err:#}"), "failed to publish printer event control message");
        } else {
            self.metrics
                .record_control_plane(ControlPlaneMetric::PublishOk);
        }
    }

    pub(crate) fn database(&self) -> &Database {
        &self.database
    }
}

pub use bootstrap::run_from_env;
pub use routes::router;
