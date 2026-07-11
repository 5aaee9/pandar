use anyhow::Context;

use crate::{AppState, artifacts, cluster, db, identity::JwtVerifier};

impl AppState {
    pub fn with_external_auth(self, verifier: JwtVerifier) -> Self {
        self.with_external_auth_option(Some(verifier))
    }

    pub fn with_bootstrap_token(self, token: impl Into<String>) -> Self {
        self.with_bootstrap_token_option(Some(token.into()))
    }

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

    pub async fn file_sqlite_for_tests() -> anyhow::Result<Self> {
        let spool_dir = tempfile::tempdir()
            .context("failed to create temporary job spool directory")?
            .keep();
        let database_dir = tempfile::tempdir()
            .context("failed to create temporary SQLite database directory")?
            .keep();
        let artifact_storage = artifacts::FilesystemArtifactStorage::new(
            spool_dir,
            artifacts::DEFAULT_MAX_ARTIFACT_BYTES,
        )?;
        let database_url = format!("sqlite://{}", database_dir.join("hub.sqlite").display());
        Self::connect_with_config_values(database_url, artifact_storage, None, None, None, None)
            .await
            .context("failed to create file SQLite test app state")
    }

    pub(crate) fn with_no_auth_for_tests(self, enabled: bool) -> Self {
        self.with_no_auth(enabled)
    }

    pub(crate) fn with_tenant_self_create_for_tests(self, allowed: bool) -> Self {
        self.with_tenant_self_create_allowed(allowed)
    }

    pub(crate) fn sibling_for_tests(&self) -> Self {
        Self::from_database_with_control_plane(
            self.database.clone(),
            self.artifact_storage.clone(),
            self.control_plane.clone(),
        )
        .with_external_auth_option(self.external_auth.clone())
        .with_no_auth(self.no_auth)
        .with_bootstrap_token_option(self.bootstrap_token.clone())
    }

    pub(crate) fn with_control_plane_for_tests(
        mut self,
        control_plane: cluster::ControlPlane,
    ) -> Self {
        self.control_plane = control_plane;
        self
    }

    pub(crate) fn with_printer_events_for_tests(
        mut self,
        printer_events: crate::printer_events::PrinterEventHub,
    ) -> Self {
        self.printer_events = printer_events;
        self
    }

    pub(crate) fn with_database_backend_for_tests(mut self, backend: db::DatabaseBackend) -> Self {
        self.database_backend_override = Some(backend);
        self
    }
}
