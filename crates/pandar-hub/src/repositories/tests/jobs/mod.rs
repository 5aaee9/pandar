use pandar_core::{AgentId, CommandId, JobId, JobStatus, PrintStatus};
use serde_json::{Value, json};

use super::*;
use crate::repositories::{ApplyPrintReport, CreatePrintJob, PrintReportDiagnostic};

mod lifecycle;
mod mapping;
mod repository;

fn create_input(
    tenant_id: pandar_core::TenantId,
    agent_id: AgentId,
    printer_id: &str,
    artifact_id: &str,
) -> CreatePrintJob {
    create_input_with_filename(tenant_id, agent_id, printer_id, artifact_id, "plate.3mf")
}

fn create_input_with_filename(
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
        plate_id: 1,
        use_ams: true,
        flow_cali: false,
        timelapse: false,
        ams_mapping_json: None,
        ams_mapping2_json: None,
    }
}

const OBSERVED_AT: &str = "2026-06-22T00:00:00Z";

fn report_input(
    tenant_id: pandar_core::TenantId,
    agent_id: AgentId,
    printer_id: &str,
    job_id: Option<JobId>,
    artifact_id: Option<String>,
    gcode_state: &str,
) -> ApplyPrintReport {
    ApplyPrintReport {
        tenant_id,
        agent_id,
        serial: format!("serial-{printer_id}"),
        job_id,
        artifact_id,
        subtask_id: None,
        gcode_file: None,
        subtask_name: None,
        gcode_state: Some(gcode_state.to_string()),
        percent: Some(42),
        remaining_time_minutes: Some(60),
        current_layer: Some(3),
        total_layers: Some(9),
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
) -> Vec<Value> {
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
    json!({
        "type": "printer_material_patch",
        "observed_at": observed_at,
        "ams_units": [{
            "unit_id": "0",
            "trays": [
                {
                    "tray_id": "0",
                    "global_tray_id": 0,
                    "filament_id": "GFL00",
                    "setting_id": "GFSL00",
                    "type": "PLA",
                    "color": "FF0000"
                },
                {
                    "tray_id": "3",
                    "global_tray_id": 11,
                    "filament_id": "GFL03",
                    "setting_id": "GFSL03",
                    "type": "ASA",
                    "color": "0000FF"
                }
            ]
        }, {
            "unit_id": "128",
            "trays": [{
                "tray_id": "0",
                "filament_id": "GFL128",
                "setting_id": "GFSL128",
                "type": "PA",
                "color": "00FFFF"
            }]
        }],
        "external_spools": [
            {
                "external_id": "254",
                "tray_id": "0",
                "filament_id": "EXT0",
                "setting_id": "EXTS0",
                "type": "PETG",
                "color": "00FF00"
            },
            {
                "external_id": "254",
                "tray_id": "1",
                "filament_id": "EXT1",
                "setting_id": "EXTS1",
                "type": "ABS",
                "color": "FFFF00"
            },
            {
                "external_id": "254",
                "tray_id": "8",
                "filament_id": "EXT8",
                "setting_id": "EXTS8",
                "type": "TPU",
                "color": "111111"
            }
        ]
    })
    .to_string()
}
