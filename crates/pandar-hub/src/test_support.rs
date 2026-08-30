use std::{ops::Deref, str::FromStr};

use anyhow::Context;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

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

pub(crate) struct PostgresTestDatabase {
    database: db::Database,
    url: String,
    schema: String,
}

impl PostgresTestDatabase {
    pub(crate) async fn new() -> Option<Self> {
        let url = match std::env::var("PANDAR_TEST_POSTGRES_URL") {
            Ok(url) => url,
            Err(err) if std::env::var("PANDAR_REQUIRE_POSTGRES_TESTS").as_deref() == Ok("true") => {
                panic!("PANDAR_TEST_POSTGRES_URL is required by this test run: {err:#}")
            }
            Err(_) => return None,
        };
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        let schema = format!("pandar_test_{}", uuid::Uuid::new_v4().simple());
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&admin)
            .await
            .unwrap();
        admin.close().await;
        let options = PgConnectOptions::from_str(&url)
            .unwrap()
            .application_name(&schema)
            .options([("search_path", schema.as_str())]);
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .unwrap();
        let database = db::Database::Postgres(pool);
        database.migrate().await.unwrap();
        Some(Self {
            database,
            url,
            schema,
        })
    }

    pub(crate) async fn reconnect(&self) -> db::Database {
        let options = PgConnectOptions::from_str(&self.url)
            .unwrap()
            .application_name(&self.schema)
            .options([("search_path", self.schema.as_str())]);
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .unwrap();
        db::Database::Postgres(pool)
    }

    pub(crate) fn schema_name(&self) -> &str {
        &self.schema
    }
}

impl Deref for PostgresTestDatabase {
    type Target = db::Database;

    fn deref(&self) -> &Self::Target {
        &self.database
    }
}

impl Drop for PostgresTestDatabase {
    fn drop(&mut self) {
        let url = self.url.clone();
        let schema = self.schema.clone();
        let cleanup = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("PostgreSQL test cleanup runtime");
            runtime.block_on(async move {
                let admin = PgPoolOptions::new()
                    .max_connections(1)
                    .connect(&url)
                    .await?;
                sqlx::query(
                    "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
                     WHERE datname = current_database() AND application_name = $1 \
                     AND pid <> pg_backend_pid()",
                )
                .bind(&schema)
                .execute(&admin)
                .await?;
                let result =
                    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
                        .execute(&admin)
                        .await;
                admin.close().await;
                result.map(|_| ())
            })
        })
        .join();
        match cleanup {
            Ok(Ok(())) => {}
            Ok(Err(err)) if !std::thread::panicking() => {
                panic!("failed to drop PostgreSQL test schema: {err:#}")
            }
            Err(_) if !std::thread::panicking() => {
                panic!("PostgreSQL test cleanup thread panicked")
            }
            _ => {}
        }
    }
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
