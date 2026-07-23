use anyhow::Context;

use crate::{AppState, artifacts, cluster, db, identity::JwtVerifier};

pub(crate) fn studio_metadata_for_tests() -> pandar_core::StudioPrintMetadata {
    pandar_core::StudioPrintMetadata::V1(pandar_core::StudioPrintMetadataV1 {
        task_name: "plate.3mf".to_owned(),
        project_name: "Test project".to_owned(),
        preset_name: "0.20mm Standard".to_owned(),
        config_plate_index: Some(1),
        nozzle_mapping: Vec::new(),
        ams_mapping: Vec::new(),
        ams_mapping2: Vec::new(),
        ams_mapping_info: Vec::new(),
        nozzles_info: Vec::new(),
        connection_type: "hub".to_owned(),
        comments: String::new(),
        origin_profile_id: 0,
        stl_design_id: 0,
        origin_model_id: String::new(),
        print_type: "from_normal".to_owned(),
        submitted_device_name: "Test printer".to_owned(),
        task_bed_leveling: false,
        task_flow_cali: false,
        task_vibration_cali: false,
        task_layer_inspect: false,
        task_record_timelapse: true,
        task_timelapse_use_internal: false,
        task_use_ams: true,
        task_bed_type: "auto".to_owned(),
        auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
        auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
        auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
        extruder_cali_manual_mode: -1,
        try_emmc_print: false,
        svc_context: String::new(),
        slicer_uid: String::new(),
    })
}

pub(crate) fn studio_submission_id_for_tests() -> pandar_core::StudioSubmissionId {
    pandar_core::StudioSubmissionId::try_from(1_i64).expect("positive Studio submission ID")
}

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
        Self::from_database_with_control_plane_and_cipher(
            self.database.clone(),
            self.artifact_storage.clone(),
            self.control_plane.clone(),
            self.printers.access_code_cipher(),
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
