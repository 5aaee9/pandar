use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};

use super::*;
use pandar_core::{AgentId, TenantId};
use requests::{
    link_printer_body, link_printer_value, link_printer_with_model_value,
    link_printer_with_serial_number_value, link_printer_with_unexpected_field_body,
    printer_ams_load_body, printer_select_extruder_body, update_printer_body,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tonic::Status;
use tracing_subscriber::fmt::MakeWriter;

use crate::protocol::agent::v1::{CameraStreamMode, HubCommand, hub_camera_command, hub_command};

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

#[tokio::test]
async fn printer_list_returns_tenant_printers() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<PrinterListResponse>(body);
    let printer = body.printers.first().unwrap();
    assert_eq!(printer.id, printer_id);
    assert_eq!(printer.tenant_id, tenant_id.to_string());
    assert_eq!(printer.agent_id, agent_id.to_string());
    assert_eq!(printer.materials, None);
}

#[tokio::test]
async fn printer_detail_returns_tenant_printer() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<PrinterResponse>(body);
    assert_eq!(body.id, printer_id);
    assert_eq!(body.tenant_id, tenant_id.to_string());
    assert_eq!(body.materials, None);
}

#[tokio::test]
async fn printer_list_and_detail_share_enriched_sanitized_print_shape() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    let applied = state
        .jobs()
        .apply_print_report(crate::repositories::ApplyPrintReport {
            tenant_id,
            agent_id,
            serial: format!("serial-{printer_id}"),
            task_id: Some("task-42".to_owned()),
            job_id: None,
            print_error: Some(83_918_929),
            printer_job_id: Some(String::new()),
            job_attr: Some(0x00b0),
            artifact_id: None,
            subtask_id: None,
            gcode_file: Some("/data/Metadata/plate_1.gcode".to_owned()),
            subtask_name: Some("Cube".to_owned()),
            gcode_state: Some("RUNNING".to_owned()),
            percent: Some(42),
            remaining_time_minutes: Some(11),
            current_layer: Some(2),
            total_layers: Some(128),
            hms: Some(vec![crate::repositories::PrinterHms {
                attr: 83_887_616,
                code: 131_184,
            }]),
            diagnostics: Vec::new(),
            printer_materials_json: String::new(),
            observed_at: "2026-07-10T01:02:03Z".to_owned(),
        })
        .await
        .unwrap();
    let expected_revision = applied.printer.unwrap().state_revision;

    let (list_status, list_body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers"),
        None,
        &token,
    )
    .await;
    let (detail_status, detail_body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(detail_status, StatusCode::OK);
    let list_json = list_body["printers"][0].clone();
    assert_eq!(list_json, detail_body);
    for private_field in ["host", "access_code"] {
        assert!(list_json.get(private_field).is_none());
    }
    for private_field in [
        "job_attr",
        "error_task_generation",
        "error_session_id",
        "error_received_at",
    ] {
        assert!(list_json["print"].get(private_field).is_none());
    }
    assert_eq!(list_json["print"]["subtask_id"], Value::Null);
    assert!(
        !serde_json::to_string(&list_json)
            .unwrap()
            .contains("SECRET")
    );

    let printer = decode::<PrinterResponse>(detail_body);
    assert_eq!(printer.state_revision, expected_revision);
    assert_eq!(printer.print.task_generation, 1);
    assert_eq!(printer.print.error_generation, 1);
    assert_eq!(printer.print.job_state, Some((0x00b0 >> 4) & 0x0f));
    assert_eq!(printer.print.gcode_state.as_deref(), Some("RUNNING"));
    assert_eq!(printer.print.task_id.as_deref(), Some("task-42"));
    assert_eq!(printer.print.subtask_id, None);
    assert_eq!(printer.print.progress_percent, Some(42));
    assert_eq!(printer.print.remaining_time_minutes, Some(11));
    assert_eq!(printer.print.current_layer, Some(2));
    assert_eq!(printer.print.total_layers, Some(128));
    assert_eq!(
        printer.print.gcode_file.as_deref(),
        Some("/data/Metadata/plate_1.gcode")
    );
    assert_eq!(printer.print.subtask_name.as_deref(), Some("Cube"));
    assert_eq!(printer.print.print_error, Some(83_918_929));
    assert_eq!(printer.print.printer_job_id.as_deref(), Some(""));
    assert_eq!(
        printer.print.hms,
        vec![crate::repositories::PrinterHms {
            attr: 83_887_616,
            code: 131_184,
        }]
    );
}

#[tokio::test]
async fn printer_camera_stream_opens_agent_camera_tunnel() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    let printer = state
        .printers()
        .get_for_tenant(tenant_id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    let (wake_sender, _wake_receiver) = tokio::sync::mpsc::channel(1);
    let (close_sender, _close_receiver) = tokio::sync::mpsc::channel(1);
    let (command_sender, mut command_receiver) = tokio::sync::mpsc::channel(1);
    state
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id,
            name: "garage".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
            command_sender,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;

    let response = raw_request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/camera.mp4"),
        &token,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "video/mp4"
    );
    let command = command_receiver.recv().await.unwrap().unwrap();
    match command.command.unwrap() {
        hub_command::Command::CameraStream(command) => match command.command.unwrap() {
            hub_camera_command::Command::Open(open) => {
                assert_eq!(open.serial_number, printer.serial_number);
                assert_eq!(open.mode, CameraStreamMode::FragmentedMp4 as i32);
            }
            other => panic!("expected open camera stream command, got {other:?}"),
        },
        other => panic!("expected camera stream command, got {other:?}"),
    }
}

#[tokio::test]
async fn tenant_admin_can_delete_printer() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode::<PrinterResponse>(body).id, printer_id);

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(decode::<PrinterListResponse>(body).printers.is_empty());

    let events = state
        .audit_events()
        .list_for_tenant(tenant_id)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "printer.delete")
        .expect("printer delete audit event");
    assert_eq!(event.target_id.as_deref(), Some(printer_id.as_str()));
}

#[tokio::test]
async fn viewer_cannot_delete_printer() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, _) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Viewer,
        "viewer-delete-printer",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
}

#[tokio::test]
async fn update_printer_updates_details_without_agent_session() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    let access_code = "UPDATED-LINK-CODE";

    let (status, body) = request_as(
        app,
        Method::PATCH,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        update_printer_body("192.168.2.11", access_code, "Office A1 Updated"),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body_text = body.to_string();
    let body = decode::<PrinterResponse>(body);
    assert_eq!(body.name, "Office A1 Updated");
    assert!(!body_text.contains(access_code));

    let printer = state
        .printers()
        .get_for_tenant(tenant_id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(printer.name, "Office A1 Updated");
    assert_eq!(printer.host.as_deref(), Some("192.168.2.11"));
    assert_eq!(printer.access_code.as_deref(), Some(access_code));

    let command = state
        .commands()
        .next_queued_for_agent(tenant_id, agent_id)
        .await
        .unwrap()
        .expect("printer update should enqueue a connection reload");
    assert_eq!(command.kind, "reload_printer_connection");
    assert_eq!(command.printer_id.as_deref(), Some(printer_id.as_str()));
    let payload: crate::repositories::ReloadPrinterConnectionPayload =
        serde_json::from_str(&command.payload_json).unwrap();
    assert_eq!(payload.printer_id, printer_id);
    assert_eq!(payload.serial_number, printer.serial_number);
    assert!(!command.payload_json.contains(access_code));
}

#[tokio::test]
async fn update_printer_keeps_existing_connection_when_fields_are_blank_without_agent_session() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    seed_printer_connection(
        state.database(),
        &printer_id,
        "192.168.2.10",
        "EXISTING-LINK-CODE",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::PATCH,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        update_printer_body(" ", "", "Office A1 Updated"),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body_text = body.to_string();
    let body = decode::<PrinterResponse>(body);
    assert_eq!(body.name, "Office A1 Updated");
    assert!(!body_text.contains("EXISTING-LINK-CODE"));
    let printer = state
        .printers()
        .get_for_tenant(tenant_id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(printer.name, "Office A1 Updated");
    assert_eq!(printer.host.as_deref(), Some("192.168.2.10"));
    assert_eq!(printer.access_code.as_deref(), Some("EXISTING-LINK-CODE"));
}

#[tokio::test]
async fn update_printer_rejects_host_change_without_access_code() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    seed_printer_connection(
        state.database(),
        &printer_id,
        "192.168.2.10",
        "EXISTING-LINK-CODE",
    )
    .await;

    let (status, _) = request_as(
        app,
        Method::PATCH,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        update_printer_body("192.168.2.11", "", "Office A1 Updated"),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let printer = state
        .printers()
        .get_for_tenant(tenant_id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(printer.host.as_deref(), Some("192.168.2.10"));
    assert_eq!(printer.access_code.as_deref(), Some("EXISTING-LINK-CODE"));
}

#[tokio::test]
async fn printer_routes_return_material_snapshots_without_credentials() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    state
        .materials()
        .upsert_from_patch(crate::repositories::MaterialPatchInput {
            tenant_id,
            agent_id,
            printer_id: printer_id.clone(),
            serial_number: "serial".to_string(),
            printer_materials_json: serde_json::to_string(&PrinterMaterialPatchFixture {
                kind: "printer_material_patch",
                observed_at: "2026-06-23T01:02:03Z",
                filament_switch_installed: true,
                ams_units: [PrinterMaterialPatchAmsUnit {
                    unit_id: "0",
                    trays: [PrinterMaterialPatchTray {
                        tray_id: "0",
                        filament_id: "GFL00",
                        material_type: "PLA",
                        color: "FF0000",
                        access_token: "secret-token",
                        auth: "secret-auth",
                        passwd: "secret-passwd",
                        access_code: "secret-access-code",
                    }],
                }],
                external_spools: [PrinterMaterialPatchExternalSpool {
                    external_id: "254",
                    tray_id: "0",
                    material_type: "PETG",
                }],
                active_tray: PrinterMaterialPatchActiveTray {
                    kind: "ams",
                    global_tray_id: 0,
                    ams_id: "0",
                    tray_id: "0",
                },
            })
            .unwrap(),
        })
        .await
        .unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.to_string().contains("secret-token"));
    assert!(!body.to_string().contains("secret-auth"));
    assert!(!body.to_string().contains("secret-passwd"));
    assert!(!body.to_string().contains("secret-access-code"));
    assert!(!body.to_string().contains("access_token"));
    assert!(!body.to_string().contains("auth"));
    assert!(!body.to_string().contains("passwd"));
    assert!(!body.to_string().contains("access_code"));
    let body = decode::<PrinterListResponse>(body);
    let materials = body.printers[0].materials.as_ref().unwrap();
    assert_eq!(materials.observed_at, "2026-06-23T01:02:03Z");
    assert_eq!(materials.filament_switch_installed, Some(true));
    assert_eq!(materials.ams_units[0].unit_id, "0");
    assert_eq!(materials.external_spools[0].external_id, "254");
    assert_eq!(materials.active_tray.as_ref().unwrap().kind, "ams");

    let (status, detail) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let detail = decode::<PrinterResponse>(detail);
    assert_eq!(detail.materials.as_ref(), Some(materials));
}

#[tokio::test]
async fn printer_control_enqueues_ams_slot_operation() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("Bambu Lab X2D"),
    )
    .await
    .unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        printer_ams_load_body(0, 1, 1, 0),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.kind, "printer_operation");
    let payload: PrinterOperationPayload = serde_json::from_str(&body.payload_json).unwrap();
    match payload.operation {
        PrinterOperation::AmsLoadFilament {
            ams_id,
            slot_id,
            global_tray_id,
            extruder_id,
        } => {
            assert_eq!(ams_id, 0);
            assert_eq!(slot_id, 1);
            assert_eq!(global_tray_id, 1);
            assert_eq!(extruder_id, 0);
        }
        other => panic!("expected ams_load_filament operation, got {other:?}"),
    }
}

#[tokio::test]
async fn tenant_printer_control_rejects_gcode_line_without_insert() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("A1"),
    )
    .await
    .unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        Some(serde_json::json!({
            "action": "gcode_line",
            "param": "M620 C1 \n",
        })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        decode::<ErrorResponse>(body).error,
        "invalid_printer_control"
    );
    assert_eq!(state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn printer_control_enqueues_select_extruder_operation() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("Bambu Lab X2D"),
    )
    .await
    .unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        printer_select_extruder_body(1),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.kind, "printer_operation");
    let payload: PrinterOperationPayload = serde_json::from_str(&body.payload_json).unwrap();
    match payload.operation {
        PrinterOperation::SelectExtruder { extruder_id } => assert_eq!(extruder_id, 1),
        other => panic!("expected select_extruder operation, got {other:?}"),
    }
}

#[tokio::test]
async fn missing_printer_detail_returns_not_found() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let printer_id = uuid::Uuid::new_v4();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(decode::<ErrorResponse>(body).error, "printer_not_found");
}

#[tokio::test]
async fn invalid_printer_id_returns_bad_request() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/not-a-uuid"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_printer_id");
}

#[tokio::test]
async fn refresh_printers_returns_command_record() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/refresh-printers"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.tenant_id, tenant_id);
    assert_eq!(body.agent_id, agent_id);
    assert_eq!(body.kind, "refresh_printers");
    assert_eq!(body.status, "queued");
    let events = state
        .audit_events()
        .list_for_tenant(TenantId::parse(&tenant_id).unwrap())
        .await
        .unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.action == "agent.refresh_printers")
    );
}

#[tokio::test]
async fn refresh_printer_materials_enqueues_for_owning_agent_and_wakes_it() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    let mut wake_receiver =
        register_route_test_session_with_wake(&state, tenant_id, agent_id).await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/materials:refresh"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.kind, "refresh_printer_materials");
    assert_eq!(body.agent_id, agent_id.to_string());
    assert_eq!(body.printer_id.as_deref(), Some(printer_id.as_str()));
    let payload: RefreshPrinterMaterialsPayload = serde_json::from_str(&body.payload_json).unwrap();
    assert_eq!(payload.printer_id, printer_id);
    assert_eq!(payload.serial_number, format!("serial-{printer_id}"));
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("agent should be woken")
        .expect("wake channel should stay open");

    let audit = state
        .audit_events()
        .list_for_tenant(tenant_id)
        .await
        .unwrap();
    assert!(
        audit
            .iter()
            .any(|event| event.action == "printer.refresh_materials")
    );
}

#[tokio::test]
async fn refresh_printer_materials_rejects_invalid_and_missing_printers() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/not-a-uuid/materials:refresh"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_printer_id");

    let missing = uuid::Uuid::new_v4();
    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{missing}/materials:refresh"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(decode::<ErrorResponse>(body).error, "printer_not_found");
}

#[tokio::test]
async fn link_printer_requires_operator_role() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, _) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;
    let token = auth_token_for_role(
        &state,
        &tenant_id,
        crate::repositories::UserRole::Viewer,
        "viewer-link-printer-token",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body("SECRET-LINK-CODE")),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
    assert_eq!(state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn link_printer_rejects_missing_local_session_without_command_row() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body("SECRET-LINK-CODE")),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(decode::<ErrorResponse>(body).error, "agent_not_connected");
    assert_eq!(state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn link_printer_missing_local_session_does_not_log_access_code() {
    let logs = CapturedLogs::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.writer())
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;
    let access_code = "SECRET-LINK-CODE";

    let _ = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body(access_code)),
        &token,
    )
    .await;
    drop(_guard);

    assert!(!logs.to_string().contains(access_code));
}

#[tokio::test]
async fn link_printer_direct_sends_secret_but_persists_only_redacted_payload() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let (command_sender, mut command_receiver) = tokio::sync::mpsc::channel(1);
    register_route_test_session(&state, tenant_id, agent_id, command_sender).await;
    let access_code = "SECRET-LINK-CODE";

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body(access_code)),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.kind, "link_printer");
    assert_eq!(body.status, "sent");
    assert!(!body.payload_json.contains(access_code));
    assert!(
        !body
            .error
            .as_deref()
            .unwrap_or_default()
            .contains(access_code)
    );

    let sent = command_receiver.recv().await.unwrap().unwrap();
    match sent.command.unwrap() {
        hub_command::Command::LinkPrinter(command) => {
            assert_eq!(command.printer_type, "BambuLab");
            assert_eq!(command.host, "192.168.2.10");
            assert_eq!(command.access_code, access_code);
            assert_eq!(command.name, "Office X1C");
        }
        other => panic!("expected link printer command, got {other:?}"),
    }

    let payload: LinkPrinterPayload = serde_json::from_str(&body.payload_json).unwrap();
    assert_eq!(payload.printer_type, "BambuLab");
    assert_eq!(payload.host, "192.168.2.10");
    assert_eq!(payload.access_code, "[redacted]");
    assert_eq!(payload.name, "Office X1C");
    assert_eq!(payload.serial_number, None);
    assert_eq!(payload.model, None);
}

#[tokio::test]
async fn link_printer_maps_absent_or_blank_optional_name_to_empty_proto_string() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let (command_sender, mut command_receiver) = tokio::sync::mpsc::channel(1);
    register_route_test_session(&state, tenant_id, agent_id, command_sender).await;

    for body in [
        link_printer_value("BambuLab", "192.168.2.10", "SECRET-LINK-CODE", None),
        link_printer_value("BambuLab", "192.168.2.11", "SECRET-LINK-CODE", Some("   ")),
    ] {
        let (status, response) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
            Some(body),
            &token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let response = decode::<CommandResponse>(response);
        let sent = command_receiver.recv().await.unwrap().unwrap();
        match sent.command.unwrap() {
            hub_command::Command::LinkPrinter(command) => {
                assert_eq!(command.name, "");
            }
            other => panic!("expected link printer command, got {other:?}"),
        }
        assert_eq!(response.status, "sent");
    }
}

#[tokio::test]
async fn link_printer_marks_command_failed_when_live_channel_closed_after_row_creation() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let (command_sender, command_receiver) = tokio::sync::mpsc::channel(1);
    drop(command_receiver);
    register_route_test_session(&state, tenant_id, agent_id, command_sender).await;
    let access_code = "SECRET-LINK-CODE";

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body(access_code)),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.kind, "link_printer");
    assert_eq!(body.status, "failed");
    assert_eq!(
        body.error.as_deref(),
        Some("agent command channel unavailable before printer link completed")
    );
    assert!(!body.payload_json.contains(access_code));
    assert!(
        !body
            .error
            .as_deref()
            .unwrap_or_default()
            .contains(access_code)
    );
    assert_eq!(state.commands().count().await.unwrap(), 1);
    let command_id = pandar_core::CommandId::parse(&body.id).unwrap();
    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, pandar_core::CommandStatus::Failed);
    assert_eq!(
        stored.error.as_deref(),
        Some("agent command channel unavailable before printer link completed")
    );
    assert!(
        !state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&command_id)
    );
}

#[tokio::test]
async fn link_printer_rejects_blank_required_fields() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;

    for body in [
        link_printer_value("BambuLab", "", "SECRET-LINK-CODE", None),
        link_printer_value("BambuLab", "192.168.2.10", "", None),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
            Some(body),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(decode::<ErrorResponse>(body).error, "bad_request");
    }

    assert_eq!(state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn link_printer_rejects_invalid_type_host_and_legacy_metadata_fields() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;

    for request in [
        link_printer_value("", "192.168.2.10", "SECRET-LINK-CODE", None),
        link_printer_value("Other", "192.168.2.10", "SECRET-LINK-CODE", None),
        link_printer_value("BambuLab", "printer.local", "SECRET-LINK-CODE", None),
        link_printer_with_serial_number_value(
            "BambuLab",
            "192.168.2.10",
            "SECRET-LINK-CODE",
            "SERIAL123",
        ),
        link_printer_with_model_value("BambuLab", "192.168.2.10", "SECRET-LINK-CODE", "X1 Carbon"),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
            Some(request),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(decode::<ErrorResponse>(body).error, "bad_request");
    }
    assert_eq!(state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn link_printer_rejects_unknown_fields() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        link_printer_with_unexpected_field_body("BambuLab", "192.168.2.10", "SECRET-LINK-CODE"),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "bad_request");
    assert_eq!(state.commands().count().await.unwrap(), 0);
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
