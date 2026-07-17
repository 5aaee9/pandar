use pandar_core::{AgentId, CommandId, JobId, JobStatus, PrintStatus};
use serde::Serialize;

use super::*;
use crate::repositories::{
    AgentArtifactAccess, ApplyPrintReport, CreatePrintJob, PrintProjectFilePayload,
    PrintReportDiagnostic,
};

pub(super) mod clear;
pub(super) mod delete;
mod lifecycle;
mod mapping;
mod recovery;
mod repository;
pub(super) mod stalled;
mod transitions;

pub(super) fn create_input(
    tenant_id: pandar_core::TenantId,
    agent_id: AgentId,
    printer_id: &str,
    artifact_id: &str,
) -> CreatePrintJob {
    create_input_with_filename(tenant_id, agent_id, printer_id, artifact_id, "plate.3mf")
}

pub(super) fn create_input_with_filename(
    tenant_id: pandar_core::TenantId,
    agent_id: AgentId,
    printer_id: &str,
    artifact_id: &str,
    filename: &str,
) -> CreatePrintJob {
    CreatePrintJob {
        tenant_id,
        printer_id: printer_id.to_string(),
        agent_id,
        artifact_id: artifact_id.to_string(),
        artifact_filename: filename.to_string(),
        artifact_content_type: "model/3mf".to_string(),
        artifact_size_bytes: 42,
        artifact_storage_path: format!("{tenant_id}/{artifact_id}/{filename}"),
        artifact_metadata_json: None,
        plate_id: 1,
        use_ams: true,
        bed_leveling: false,
        auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
        flow_cali: false,
        auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
        auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
        timelapse: false,
        ams_mapping_json: None,
        ams_mapping2_json: None,
        ams_mapping_info_json: None,
    }
}

const OBSERVED_AT: &str = "2026-06-22T00:00:00Z";

pub(super) fn report_input(
    tenant_id: pandar_core::TenantId,
    agent_id: AgentId,
    printer_id: &str,
    job_id: Option<JobId>,
    artifact_id: Option<String>,
    gcode_state: &str,
) -> ApplyPrintReport {
    let task_id = job_id.as_ref().map(ToString::to_string);
    ApplyPrintReport {
        tenant_id,
        agent_id,
        serial: format!("serial-{printer_id}"),
        task_id,
        job_id,
        print_error: None,
        printer_job_id: None,
        job_attr: None,
        artifact_id,
        subtask_id: None,
        gcode_file: None,
        subtask_name: None,
        gcode_state: Some(gcode_state.to_string()),
        percent: Some(42),
        remaining_time_minutes: Some(60),
        current_layer: Some(3),
        total_layers: Some(9),
        hms: None,
        diagnostics: Vec::new(),
        printer_materials_json: String::new(),
        observed_at: OBSERVED_AT.to_string(),
    }
}

fn diagnostic(kind: &str, code: &str, message: &str) -> PrintReportDiagnostic {
    PrintReportDiagnostic {
        kind: kind.to_string(),
        severity: if kind == "print_error" {
            "error".to_string()
        } else {
            "warning".to_string()
        },
        code: Some(code.to_string()),
        message: message.to_string(),
        payload_json: format!(r#"{{"code":"{code}","message":"{message}"}}"#),
    }
}

async fn machine_event_count(database: &Database) -> i64 {
    let Database::Sqlite(pool) = database else {
        panic!("expected SQLite database");
    };
    sqlx::query_scalar("SELECT COUNT(*) FROM machine_events")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn printer_level_machine_event_count(database: &Database) -> i64 {
    let Database::Sqlite(pool) = database else {
        panic!("expected SQLite database");
    };
    sqlx::query_scalar("SELECT COUNT(*) FROM machine_events WHERE job_id IS NULL")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn material_snapshot_count(database: &Database) -> i64 {
    let Database::Sqlite(pool) = database else {
        panic!("expected SQLite database");
    };
    sqlx::query_scalar("SELECT COUNT(*) FROM printer_material_snapshots")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn queued_payloads(
    commands: &crate::repositories::CommandRepository,
    tenant_id: pandar_core::TenantId,
    agent_id: AgentId,
) -> Vec<PrintProjectFilePayload> {
    let mut payloads = Vec::new();
    while let Some(command) = commands
        .next_queued_for_agent(tenant_id, agent_id)
        .await
        .unwrap()
    {
        payloads.push(serde_json::from_str(&command.payload_json).unwrap());
        commands
            .mark_sent(command.id, tenant_id, agent_id)
            .await
            .unwrap();
    }
    payloads
}

fn material_patch_json(observed_at: &str) -> String {
    serde_json::to_string(&TestMaterialPatch {
        kind: "printer_material_patch",
        observed_at,
        ams_units: vec![
            TestAmsUnit {
                unit_id: "0",
                trays: vec![
                    TestMaterialTray {
                        tray_id: "0",
                        global_tray_id: Some(0),
                        filament_id: "GFL00",
                        setting_id: "GFSL00",
                        material_type: "PLA",
                        color: "FF0000",
                    },
                    TestMaterialTray {
                        tray_id: "3",
                        global_tray_id: Some(11),
                        filament_id: "GFL03",
                        setting_id: "GFSL03",
                        material_type: "ASA",
                        color: "0000FF",
                    },
                ],
            },
            TestAmsUnit {
                unit_id: "128",
                trays: vec![TestMaterialTray {
                    tray_id: "0",
                    global_tray_id: None,
                    filament_id: "GFL128",
                    setting_id: "GFSL128",
                    material_type: "PA",
                    color: "00FFFF",
                }],
            },
        ],
        external_spools: vec![
            TestExternalSpool {
                external_id: "254",
                tray_id: "0",
                filament_id: "EXT0",
                setting_id: "EXTS0",
                material_type: "PETG",
                color: "00FF00",
            },
            TestExternalSpool {
                external_id: "254",
                tray_id: "1",
                filament_id: "EXT1",
                setting_id: "EXTS1",
                material_type: "ABS",
                color: "FFFF00",
            },
            TestExternalSpool {
                external_id: "254",
                tray_id: "8",
                filament_id: "EXT8",
                setting_id: "EXTS8",
                material_type: "TPU",
                color: "111111",
            },
        ],
    })
    .unwrap()
}

pub(super) fn artifact_metadata_json(display_name: &str, default_plate_id: u32) -> String {
    serde_json::to_string(&TestArtifactMetadata {
        source: "bambu_3mf",
        display_name,
        default_plate_id,
        plate_count: 1,
        plates: Vec::<TestPlateMetadata>::new(),
        warnings: Vec::<String>::new(),
    })
    .unwrap()
}

#[derive(Serialize)]
struct TestMaterialPatch<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    observed_at: &'a str,
    ams_units: Vec<TestAmsUnit>,
    external_spools: Vec<TestExternalSpool>,
}

#[derive(Serialize)]
struct TestAmsUnit {
    unit_id: &'static str,
    trays: Vec<TestMaterialTray>,
}

#[derive(Serialize)]
struct TestMaterialTray {
    tray_id: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    global_tray_id: Option<u32>,
    filament_id: &'static str,
    setting_id: &'static str,
    #[serde(rename = "type")]
    material_type: &'static str,
    color: &'static str,
}

#[derive(Serialize)]
struct TestExternalSpool {
    external_id: &'static str,
    tray_id: &'static str,
    filament_id: &'static str,
    setting_id: &'static str,
    #[serde(rename = "type")]
    material_type: &'static str,
    color: &'static str,
}

#[derive(Serialize)]
struct TestArtifactMetadata<'a> {
    source: &'static str,
    display_name: &'a str,
    default_plate_id: u32,
    plate_count: u32,
    plates: Vec<TestPlateMetadata>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct TestPlateMetadata;
