use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};

use super::*;
use pandar_core::{AgentId, DiagnosticCompatibility, TenantId};
use requests::{
    link_printer_body, link_printer_value, link_printer_with_model_value,
    link_printer_with_serial_number_value, link_printer_with_unexpected_field_body,
    printer_ams_load_body, printer_ams_start_drying_body, printer_ams_stop_drying_body,
    printer_select_extruder_body, update_printer_body,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tonic::Status;
use tracing_subscriber::fmt::MakeWriter;

use pandar_protocol::agent::v1::{CameraStreamMode, HubCommand, hub_camera_command, hub_command};

mod controls;
mod details;
mod h2c;
mod link;
mod management;
mod read;
mod requests;

#[derive(Debug, Deserialize)]
struct TenantResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct AgentResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct PrinterListResponse {
    printers: Vec<PrinterResponse>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct PrinterResponse {
    id: String,
    tenant_id: String,
    agent_id: String,
    name: String,
    compatibility: DiagnosticCompatibility,
    materials: Option<PrinterMaterialsResponse>,
    state_revision: u64,
    print: EnrichedPrint,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct EnrichedPrint {
    task_generation: u64,
    error_generation: u64,
    hms: Vec<crate::repositories::PrinterHms>,
    job_state: Option<u32>,
    gcode_state: Option<String>,
    task_id: Option<String>,
    subtask_id: Option<String>,
    progress_percent: Option<u8>,
    speed_level: Option<u8>,
    remaining_time_minutes: Option<u32>,
    current_layer: Option<u32>,
    total_layers: Option<u32>,
    gcode_file: Option<String>,
    subtask_name: Option<String>,
    print_error: Option<u32>,
    printer_job_id: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct PrinterMaterialsResponse {
    filament_switch_installed: Option<bool>,
    ams_units: Vec<AmsUnitResponse>,
    external_spools: Vec<ExternalSpoolResponse>,
    active_tray: Option<ActiveTrayResponse>,
    observed_at: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct AmsUnitResponse {
    unit_id: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct ExternalSpoolResponse {
    external_id: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct ActiveTrayResponse {
    kind: String,
}

#[derive(Debug, Deserialize)]
struct CommandResponse {
    id: String,
    tenant_id: String,
    agent_id: String,
    printer_id: Option<String>,
    kind: String,
    status: String,
    payload_json: String,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PrinterOperationPayload {
    operation: PrinterOperation,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum PrinterOperation {
    #[serde(rename = "ams_load_filament")]
    AmsLoadFilament {
        ams_id: u32,
        slot_id: u32,
        global_tray_id: u32,
        extruder_id: u32,
    },
    #[serde(rename = "ams_start_drying")]
    AmsStartDrying {
        ams_id: u32,
        temperature_celsius: u16,
        duration_hours: u16,
        filament: String,
        rotate_tray: bool,
    },
    #[serde(rename = "ams_stop_drying")]
    AmsStopDrying { ams_id: u32 },
    #[serde(rename = "select_extruder")]
    SelectExtruder { extruder_id: u32 },
}

#[derive(Debug, Deserialize)]
struct RefreshPrinterMaterialsPayload {
    printer_id: String,
    serial_number: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkPrinterPayload {
    printer_type: String,
    host: String,
    access_code: String,
    name: String,
    serial_number: Option<String>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> T {
    decode_json(value)
}

#[derive(Debug, Serialize)]
struct PrinterMaterialPatchFixture {
    #[serde(rename = "type")]
    kind: &'static str,
    observed_at: &'static str,
    filament_switch_installed: bool,
    ams_units: [PrinterMaterialPatchAmsUnit; 1],
    external_spools: [PrinterMaterialPatchExternalSpool; 1],
    active_tray: PrinterMaterialPatchActiveTray,
}

#[derive(Debug, Serialize)]
struct PrinterMaterialPatchAmsUnit {
    unit_id: &'static str,
    trays: [PrinterMaterialPatchTray; 1],
}

#[derive(Debug, Serialize)]
struct PrinterMaterialPatchTray {
    tray_id: &'static str,
    filament_id: &'static str,
    #[serde(rename = "type")]
    material_type: &'static str,
    color: &'static str,
    access_token: &'static str,
    auth: &'static str,
    passwd: &'static str,
    access_code: &'static str,
}

#[derive(Debug, Serialize)]
struct PrinterMaterialPatchExternalSpool {
    external_id: &'static str,
    tray_id: &'static str,
    #[serde(rename = "type")]
    material_type: &'static str,
}

#[derive(Debug, Serialize)]
struct PrinterMaterialPatchActiveTray {
    kind: &'static str,
    global_tray_id: u8,
    ams_id: &'static str,
    tray_id: &'static str,
}

async fn seed_printer_connection(
    database: &crate::db::Database,
    printer_id: &str,
    host: &str,
    access_code: &str,
) {
    let (tenant_id, serial_number): (String, String) = match database {
        crate::db::Database::Sqlite(pool) => {
            sqlx::query_as("SELECT tenant_id, serial_number FROM printers WHERE id = ?1")
                .bind(printer_id)
                .fetch_one(pool)
                .await
                .unwrap()
        }
        crate::db::Database::Postgres(pool) => {
            sqlx::query_as("SELECT tenant_id, serial_number FROM printers WHERE id = $1")
                .bind(printer_id)
                .fetch_one(pool)
                .await
                .unwrap()
        }
    };
    let encrypted = crate::printer_secrets::configured_printer_access_code_cipher()
        .unwrap()
        .encrypt(&tenant_id, &serial_number, access_code)
        .unwrap();
    match database {
        crate::db::Database::Sqlite(pool) => {
            sqlx::query(
                "UPDATE printers SET host = ?1, access_code = NULL, access_code_encrypted = ?2 WHERE id = ?3",
            )
            .bind(host)
            .bind(encrypted)
            .bind(printer_id)
            .execute(pool)
            .await
            .unwrap();
        }
        crate::db::Database::Postgres(pool) => {
            sqlx::query(
                "UPDATE printers SET host = $1, access_code = NULL, access_code_encrypted = $2 WHERE id = $3",
            )
            .bind(host)
            .bind(encrypted)
            .bind(printer_id)
            .execute(pool)
            .await
            .unwrap();
        }
    }
}

async fn register_route_test_session(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    command_sender: mpsc::Sender<Result<HubCommand, Status>>,
) {
    state
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id,
            name: "shop-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender: mpsc::channel(1).0,
            close_sender: mpsc::channel(1).0,
            command_sender,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;
}

async fn register_route_test_session_with_wake(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> mpsc::Receiver<()> {
    let (wake_sender, wake_receiver) = mpsc::channel(1);
    state
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id,
            name: "shop-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender: mpsc::channel(1).0,
            command_sender: mpsc::channel(1).0,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;
    wake_receiver
}

#[derive(Clone)]
struct CapturedLogs {
    output: Arc<Mutex<Vec<u8>>>,
}

impl CapturedLogs {
    fn new() -> Self {
        Self {
            output: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn writer(&self) -> TestLogWriter {
        TestLogWriter {
            output: self.output.clone(),
        }
    }
}

impl std::fmt::Display for CapturedLogs {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let output = self.output.lock().unwrap().clone();
        formatter.write_str(&String::from_utf8_lossy(&output))
    }
}

#[derive(Clone)]
struct TestLogWriter {
    output: Arc<Mutex<Vec<u8>>>,
}

impl<'writer> MakeWriter<'writer> for TestLogWriter {
    type Writer = TestLogBuffer;

    fn make_writer(&'writer self) -> Self::Writer {
        TestLogBuffer {
            output: self.output.clone(),
        }
    }
}

struct TestLogBuffer {
    output: Arc<Mutex<Vec<u8>>>,
}

impl Write for TestLogBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.output.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
