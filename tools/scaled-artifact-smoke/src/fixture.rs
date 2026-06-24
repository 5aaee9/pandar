use std::sync::Arc;

use anyhow::Context;
use pandar_hub::{
    AppState,
    artifacts::ArtifactStorage,
    cluster::ControlPlane,
    db::{Database, DatabaseConfig},
    protocol::agent::v1::PrintJobReport,
    repositories::{
        ApplyPrintReport, AuditActor, PrinterSnapshotUpsert, TenantTokenScope, UserRole,
    },
};
use tempfile::TempDir;

use crate::storage::SharedObjectStorage;

pub const ARTIFACT_BYTES: &[u8] = b"scaled smoke 3mf bytes";

pub struct SmokeWorld {
    pub temp: TempDir,
    pub database: Database,
    pub storage: Arc<dyn ArtifactStorage>,
    pub control_plane: ControlPlane,
    pub hub_a: AppState,
    pub hub_b: AppState,
}

impl SmokeWorld {
    pub async fn new() -> anyhow::Result<Self> {
        let temp = tempfile::tempdir().context("create smoke temp dir")?;
        let database_url = format!("sqlite://{}", temp.path().join("hub.sqlite").display());
        let database = Database::connect(&DatabaseConfig::from_url(database_url)?).await?;
        database.migrate().await?;

        let storage: Arc<dyn ArtifactStorage> =
            Arc::new(SharedObjectStorage::new(temp.path().join("objects"))?);
        let control_plane = ControlPlane::in_process();
        let hub_a = AppState::from_database_with_control_plane(
            database.clone(),
            storage.clone(),
            control_plane.clone(),
        );
        let hub_b = AppState::from_database_with_control_plane(
            database.clone(),
            storage.clone(),
            control_plane.clone(),
        );

        Ok(Self {
            temp,
            database,
            storage,
            control_plane,
            hub_a,
            hub_b,
        })
    }

    pub fn restarted_state(&self) -> AppState {
        AppState::from_database_with_control_plane(
            self.database.clone(),
            self.storage.clone(),
            self.control_plane.clone(),
        )
    }
}

pub async fn world_with_storage(storage: Arc<dyn ArtifactStorage>) -> anyhow::Result<SmokeWorld> {
    let temp = tempfile::tempdir().context("create smoke temp dir")?;
    let database_url = format!("sqlite://{}", temp.path().join("hub.sqlite").display());
    let database = Database::connect(&DatabaseConfig::from_url(database_url)?).await?;
    database.migrate().await?;
    let control_plane = ControlPlane::in_process();
    let hub_a = AppState::from_database_with_control_plane(
        database.clone(),
        storage.clone(),
        control_plane.clone(),
    );
    let hub_b = AppState::from_database_with_control_plane(
        database.clone(),
        storage.clone(),
        control_plane.clone(),
    );

    Ok(SmokeWorld {
        temp,
        database,
        storage,
        control_plane,
        hub_a,
        hub_b,
    })
}

pub async fn seed_fixture(state: &AppState, suffix: &str) -> anyhow::Result<SmokeFixture> {
    let tenant = state
        .tenants()
        .create(
            &format!("scaled-smoke-{suffix}"),
            &format!("Scaled Smoke {suffix}"),
        )
        .await?;
    let admin = state
        .auth()
        .create_user(
            tenant.id,
            &format!("scaled-smoke-{suffix}@example.invalid"),
            "Scaled Smoke",
            UserRole::TenantAdmin,
        )
        .await?;
    let agent = state
        .agents()
        .create(tenant.id, &format!("scaled-smoke-agent-{suffix}"))
        .await?;
    let agent_credential = format!("pandar_agent_scaled_smoke_secret_{suffix}");
    state
        .agents()
        .rotate_credential(
            tenant.id,
            agent.id,
            &agent_credential,
            AuditActor::user(admin.id.clone()),
        )
        .await?;
    let printer = state
        .printers()
        .upsert_snapshot(
            tenant.id,
            agent.id,
            PrinterSnapshotUpsert {
                serial_number: format!("scaled-smoke-serial-{suffix}"),
                name: format!("Scaled Smoke Printer {suffix}"),
                model: Some("X1C".to_owned()),
                status: "online".to_owned(),
                observed_at: pandar_core::created_at_now(),
            },
        )
        .await?;
    let plugin_token = state
        .auth()
        .create_tenant_token_with_audit(
            tenant.id,
            &format!("scaled smoke plugin {suffix}"),
            vec![TenantTokenScope::PluginStudio],
            None,
            AuditActor::user(admin.id.clone()),
        )
        .await?;
    let tenant_token = state
        .auth()
        .create_tenant_token_with_audit(
            tenant.id,
            &format!("scaled smoke tenant {suffix}"),
            vec![TenantTokenScope::All],
            None,
            AuditActor::user(admin.id),
        )
        .await?;

    Ok(SmokeFixture {
        tenant_id: tenant.id,
        agent_id: agent.id,
        printer_id: printer.id,
        printer_serial: printer.serial_number,
        plugin_token: plugin_token.plaintext_token,
        tenant_token: tenant_token.plaintext_token,
        agent_credential,
    })
}

pub fn report_input(
    fixture: &SmokeFixture,
    job_id: Option<pandar_core::JobId>,
    artifact_id: Option<String>,
    gcode_state: &str,
) -> ApplyPrintReport {
    ApplyPrintReport {
        tenant_id: fixture.tenant_id,
        agent_id: fixture.agent_id,
        serial: fixture.printer_serial.clone(),
        job_id,
        artifact_id,
        subtask_id: None,
        gcode_file: None,
        subtask_name: None,
        gcode_state: Some(gcode_state.to_owned()),
        percent: Some(if gcode_state == "FINISH" { 100 } else { 42 }),
        remaining_time_minutes: Some(if gcode_state == "FINISH" { 0 } else { 60 }),
        current_layer: Some(3),
        total_layers: Some(9),
        diagnostics: Vec::new(),
        printer_materials_json: String::new(),
        observed_at: "2026-06-24T00:00:00Z".to_owned(),
    }
}

pub fn report(
    fixture: &SmokeFixture,
    job_id: Option<pandar_core::JobId>,
    artifact_id: Option<String>,
    gcode_state: &str,
) -> PrintJobReport {
    PrintJobReport {
        job_id: job_id.map(|id| id.to_string()).unwrap_or_default(),
        artifact_id: artifact_id.unwrap_or_default(),
        subtask_id: String::new(),
        serial: fixture.printer_serial.clone(),
        gcode_file: String::new(),
        subtask_name: String::new(),
        gcode_state: gcode_state.to_owned(),
        percent: if gcode_state == "FINISH" { 100 } else { 42 },
        has_percent: true,
        remaining_time_minutes: if gcode_state == "FINISH" { 0 } else { 60 },
        has_remaining_time_minutes: true,
        current_layer: 3,
        has_current_layer: true,
        total_layers: 9,
        has_total_layers: true,
        diagnostics: Vec::new(),
        printer_materials_json: String::new(),
        observed_at: "2026-06-24T00:00:00Z".to_owned(),
    }
}

pub struct SmokeFixture {
    pub tenant_id: pandar_core::TenantId,
    pub agent_id: pandar_core::AgentId,
    pub printer_id: String,
    pub printer_serial: String,
    pub plugin_token: String,
    pub tenant_token: String,
    pub agent_credential: String,
}
