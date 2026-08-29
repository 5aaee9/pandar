mod artifacts;
mod config;
mod diagnostics;
mod fake_gateway;
mod firmware_wire;
mod link_cases;
mod link_support;
mod operation_basic;
mod operation_dispatch;
mod operation_features;
mod operation_lights;
mod operation_missing;
mod operation_rack;
mod operation_support;
mod operation_thermal;
mod operation_validation;
mod print;
mod print_error;
mod refresh_basic;
mod refresh_materials;
mod reload_connection;
mod reports;

use std::{sync::Arc, time::Duration};

use anyhow::Context;
use async_trait::async_trait;
use link_support::*;
use operation_support::*;
use pandar_core::{BambuDeviceFeature, BambuDeviceFeatures, PrinterFirmwareModule};
use reports::{ams_ready_report, get_version_report};
use serde::{Deserialize, Serialize};
use tokio::{sync::Mutex, sync::mpsc};

use super::handle_non_firmware_command_with_gateway as handle_command_with_gateway;
use super::*;
use crate::machine::{
    BambuMachineGateway, BambuPrinterEndpoint, FirmwareObservationCache, MachineNozzleTemperature,
    MachineSnapshot, MaterialRefreshResult, NoopMachineGateway, PrintProjectDispatchResult,
    PrinterOperation as MachinePrinterOperation, PrinterRefreshResult,
    diagnostics::PrinterDiagnosticResult,
    discovery::{DiscoveredPrinter, PrinterDiscoveryResult},
    file_transfer::FakeMachineFileTransfer,
    mqtt::FakeMqttTransport,
    runtime::test_support::TestRuntimeBambuMachineGateway,
};
use pandar_protocol::agent::v1::{
    AmsLoadFilamentOperation, AmsRereadRfidOperation, AmsStartDryingOperation,
    AmsStopDryingOperation, AmsUnloadFilamentOperation, Axis, AxisMovement, DeviceFeature,
    DiagnosePrinter, DiscoverPrinters, GcodeLineOperation, HolderNozzleRefreshOperation,
    HomeOperation, HubCommand, LinkPrinter, MoveAxesOperation, NozzleHolderCtrlOperation,
    NozzleInfoConfirmOperation, PauseOperation, PrinterOperation as ProtoPrinterOperation,
    RefreshPrinterMaterials, RefreshPrinters, SelectExtruderOperation, SetBedTemperatureOperation,
    SetChamberLightOperation, SetChamberTemperatureOperation, SetFanSpeedOperation,
    SetHotendTemperatureOperation, SetPrintSpeedOperation, ToggleLightOperation, printer_operation,
};

fn refresh_command(command_id: String) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::RefreshPrinters(RefreshPrinters {})),
    }
}

fn refresh_materials_command(
    command_id: String,
    printer_id: &str,
    serial_number: &str,
) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::RefreshPrinterMaterials(
            RefreshPrinterMaterials {
                printer_id: printer_id.to_owned(),
                serial_number: serial_number.to_owned(),
            },
        )),
    }
}

async fn drain_until_success(receiver: &mut mpsc::Receiver<AgentEvent>) {
    while let Some(event) = receiver.recv().await {
        if matches!(event.event, Some(agent_event::Event::CommandResult(result)) if result.success)
        {
            return;
        }
    }
    panic!("expected success event");
}

fn link_printer_command(command_id: String, access_code: &str) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::LinkPrinter(LinkPrinter {
            host: "192.0.2.10".to_owned(),
            access_code: access_code.to_owned(),
            name: "Office X1C".to_owned(),
            printer_type: "BambuLab".to_owned(),
        })),
    }
}

pub(super) fn discover_command(command_id: String) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::DiscoverPrinters(DiscoverPrinters {
            timeout_seconds: 1,
        })),
    }
}

pub(super) fn diagnose_command(command_id: String, serial_number: &str) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::DiagnosePrinter(DiagnosePrinter {
            serial_number: serial_number.to_owned(),
        })),
    }
}

pub(super) fn test_config() -> AgentConfig {
    AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: ".".into(),
    }
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestPrinterLinkResult {
    #[serde(rename = "type")]
    kind: String,
    serial_number: String,
    host: String,
    name: String,
    model: String,
    status: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestPrinterLinkFailure {
    #[serde(rename = "type")]
    kind: String,
    error_code: String,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
struct TestPrinterOperationResult {
    #[serde(rename = "type")]
    kind: String,
    action: String,
    serial_number: String,
    speed_mode: Option<u8>,
    fan_index: Option<u8>,
    speed_percent: Option<u8>,
    airduct: Option<bool>,
    extruder_id: Option<u32>,
    x_mm: Option<f64>,
    y_mm: Option<f64>,
    z_mm: Option<f64>,
    feedrate_mm_per_min: Option<f64>,
    temperature_celsius: Option<u16>,
    wait: Option<bool>,
    light_on: Option<bool>,
    ams_id: Option<u32>,
    slot_id: Option<u32>,
    holder_action: Option<u32>,
    nozzle_id: Option<u32>,
    duration_hours: Option<u16>,
    filament: Option<String>,
    rotate_tray: Option<bool>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestMaterialPatch {
    #[serde(rename = "type")]
    kind: String,
    ams_units: Vec<TestMaterialPatchAmsUnit>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestMaterialPatchAmsUnit {
    trays: Vec<TestMaterialPatchTray>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestMaterialPatchTray {
    #[serde(rename = "type")]
    filament_type: String,
}

fn link_result(result_json: &str) -> TestPrinterLinkResult {
    serde_json::from_str(result_json).unwrap()
}

fn link_failure(result_json: &str) -> TestPrinterLinkFailure {
    serde_json::from_str(result_json).unwrap()
}

fn operation_result(result_json: &str) -> TestPrinterOperationResult {
    serde_json::from_str(result_json).unwrap()
}

fn empty_operation_result() -> TestPrinterOperationResult {
    TestPrinterOperationResult::default()
}

fn material_patch(result_json: &str) -> TestMaterialPatch {
    serde_json::from_str(result_json).unwrap()
}

fn snapshot(serial: &str, name: &str, model: Option<&str>, state: &str) -> MachineSnapshot {
    MachineSnapshot {
        serial: serial.to_owned(),
        host: Some("192.0.2.10".to_owned()),
        access_code: Some("12345678".to_owned()),
        name: name.to_owned(),
        model: model.map(str::to_owned),
        state: Some(state.to_owned()),
        nozzle_temperatures: Vec::new(),
        active_nozzle: None,
        bed_temperature_celsius: None,
        bed_target_temperature_celsius: None,
        chamber_temperature_celsius: None,
        chamber_target_temperature_celsius: None,
        chamber_light_on: None,
        cooling_system: None,
        device_features: None,
        device_features2: None,
        nozzle_system: None,
        telemetry_authoritative: true,
    }
}

#[test]
fn command_and_mqtt_snapshot_events_share_the_machine_projection() {
    let mut snapshot = snapshot("SERIAL1", "office", Some("X1 Carbon"), "RUNNING");
    snapshot.nozzle_temperatures = vec![MachineNozzleTemperature {
        label: Some("left".to_owned()),
        current_celsius: Some("215".to_owned()),
        target_celsius: Some("220".to_owned()),
        diameter_mm: Some("0.4".to_owned()),
        nozzle_type: Some("hardened_steel".to_owned()),
        snow: Some(1),
        hnow: Some(2),
    }];
    snapshot.active_nozzle = Some("left".to_owned());
    snapshot.bed_temperature_celsius = Some("55".to_owned());
    snapshot.bed_target_temperature_celsius = Some("60".to_owned());
    snapshot.chamber_temperature_celsius = Some("35".to_owned());
    snapshot.chamber_target_temperature_celsius = Some("40".to_owned());
    snapshot.chamber_light_on = Some(true);
    snapshot.device_features = Some(BambuDeviceFeatures::from_bits(0x41));

    let command = responses::printer_snapshot_event(&test_config(), snapshot.clone());
    let mqtt = crate::machine::mqtt::printer_snapshot_event(&test_config(), snapshot.clone());
    let authoritative = authoritative_printer_snapshot_event(&test_config(), snapshot);
    let Some(agent_event::Event::PrinterSnapshot(command)) = command.event else {
        panic!("expected command printer snapshot");
    };
    let Some(agent_event::Event::PrinterSnapshot(mqtt)) = mqtt.event else {
        panic!("expected MQTT printer snapshot");
    };
    let Some(agent_event::Event::PrinterSnapshot(mut authoritative)) = authoritative.event else {
        panic!("expected authoritative printer snapshot");
    };

    assert_eq!(command, mqtt);
    assert!(!command.connection_authoritative);
    assert!(authoritative.connection_authoritative);
    authoritative.connection_authoritative = false;
    assert_eq!(command, authoritative);
}

fn assert_snapshot(event: AgentEvent, serial: &str, name: &str, model: &str, state: &str) {
    assert_eq!(event.agent_id, "agent-id");
    assert_eq!(event.tenant_id, "tenant-id");
    match event.event.unwrap() {
        agent_event::Event::PrinterSnapshot(snapshot) => {
            assert_eq!(snapshot.serial, serial);
            assert_eq!(snapshot.name, name);
            assert_eq!(snapshot.model, model);
            assert_eq!(snapshot.state, state);
        }
        other => panic!("expected printer snapshot, got {other:?}"),
    }
}

fn assert_material_snapshot(event: AgentEvent, serial: &str, printer_id: Option<&str>) {
    assert_eq!(event.agent_id, "agent-id");
    assert_eq!(event.tenant_id, "tenant-id");
    match event.event.unwrap() {
        agent_event::Event::PrinterMaterialsSnapshot(snapshot) => {
            assert_eq!(snapshot.serial, serial);
            assert_eq!(snapshot.printer_id, printer_id.unwrap_or_default());
            assert_eq!(
                material_patch(&snapshot.printer_materials_json),
                TestMaterialPatch {
                    kind: "printer_material_patch".to_owned(),
                    ams_units: vec![TestMaterialPatchAmsUnit {
                        trays: vec![TestMaterialPatchTray {
                            filament_type: "PLA".to_owned(),
                        }],
                    }],
                }
            );
        }
        other => panic!("expected printer materials snapshot, got {other:?}"),
    }
}

fn refresh_result(
    snapshot: MachineSnapshot,
    materials: MaterialRefreshResult,
) -> PrinterRefreshResult {
    PrinterRefreshResult {
        snapshot,
        materials: Some(materials),
    }
}

fn material_result(serial: &str, printer_id: Option<&str>) -> MaterialRefreshResult {
    MaterialRefreshResult {
        serial: serial.to_owned(),
        printer_id: printer_id.map(str::to_owned),
        printer_materials_json: serde_json::to_string(&TestMaterialPatchResult {
            kind: "printer_material_patch",
            observed_at: "2026-07-02T00:00:00Z",
            ams_units: [TestMaterialPatchResultAmsUnit {
                unit_id: "0",
                trays: [TestMaterialPatchResultTray {
                    tray_id: "0",
                    filament_type: "PLA",
                }],
            }],
            external_spools: [],
        })
        .unwrap(),
    }
}

#[derive(Debug, Serialize)]
struct TestMaterialPatchResult {
    #[serde(rename = "type")]
    kind: &'static str,
    observed_at: &'static str,
    ams_units: [TestMaterialPatchResultAmsUnit; 1],
    external_spools: [(); 0],
}

#[derive(Debug, Serialize)]
struct TestMaterialPatchResultAmsUnit {
    unit_id: &'static str,
    trays: [TestMaterialPatchResultTray; 1],
}

#[derive(Debug, Serialize)]
struct TestMaterialPatchResultTray {
    tray_id: &'static str,
    #[serde(rename = "type")]
    filament_type: &'static str,
}

pub(super) fn assert_failure_contains(event: AgentEvent, command_id: &str, needle: &str) {
    match event.event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert_eq!(result.command_id, command_id);
            assert!(!result.success);
            assert!(result.error.contains(needle), "{}", result.error);
        }
        other => panic!("expected command result, got {other:?}"),
    }
}

#[derive(Debug, Clone)]
pub(super) struct FakeGateway {
    result: Arc<Mutex<anyhow::Result<Vec<PrinterRefreshResult>>>>,
    material_result: Arc<Mutex<anyhow::Result<MaterialRefreshResult>>>,
    access_code: Option<String>,
}
