use std::collections::BTreeMap;

use serde::Serialize;
use tonic::Code;

use super::*;
use crate::{
    printer_events::{PrinterEvent, PrinterEventMaterialJson},
    repositories::{MaterialPatchInput, test_helpers::insert_printer_fixture},
};
use pandar_protocol::agent::v1::{
    PrinterDeviceFeatures, PrinterDeviceFeaturesSnapshot, PrinterSnapshot,
};

mod material_reset;

mod materials;
mod session;
mod telemetry;

pub(super) fn snapshot(serial: &str, name: &str, model: &str, state: &str) -> PrinterSnapshot {
    PrinterSnapshot {
        serial: serial.to_string(),
        host: "192.0.2.10".to_string(),
        access_code: "12345678".to_string(),
        name: name.to_string(),
        model: model.to_string(),
        state: state.to_string(),
        nozzle_temperatures: Vec::new(),
        active_nozzle: String::new(),
        bed_temperature_celsius: String::new(),
        bed_target_temperature_celsius: String::new(),
        chamber_temperature_celsius: String::new(),
        chamber_target_temperature_celsius: String::new(),
        chamber_light_on: None,
        cooling_system: None,
        device_features: None,
        connection_authoritative: false,
        telemetry_authoritative: false,
        nozzle_system: None,
    }
}

pub(super) fn snapshot_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    snapshot: PrinterSnapshot,
) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_string(),
        event: Some(agent_event::Event::PrinterSnapshot(snapshot)),
    }
}

fn device_features_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    serial: &str,
    device_features: Option<PrinterDeviceFeatures>,
) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "device-features-event".to_owned(),
        event: Some(agent_event::Event::PrinterDeviceFeaturesSnapshot(
            PrinterDeviceFeaturesSnapshot {
                serial: serial.to_owned(),
                device_features,
            },
        )),
    }
}

pub(super) fn valid_material_patch(observed_at: &str) -> String {
    serde_json::to_string(&TestMaterialPatch {
        kind: "printer_material_patch",
        observed_at,
        filament_switch_installed: true,
        cfg: "8000000000000001",
        aux: "A4003001",
        stat: "1000000001",
        ams_units: vec![TestAmsUnit {
            unit_id: "0",
            info: "00000E00",
            trays: vec![TestMaterialPatchTray {
                tray_id: "0",
                material_type: "PLA",
            }],
        }],
        external_spools: Vec::<TestExternalSpool>::new(),
    })
    .unwrap()
}

#[derive(Serialize)]
struct TestMaterialPatch<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    observed_at: &'a str,
    filament_switch_installed: bool,
    cfg: &'static str,
    aux: &'static str,
    stat: &'static str,
    ams_units: Vec<TestAmsUnit>,
    external_spools: Vec<TestExternalSpool>,
}

#[derive(Serialize)]
struct TestAmsUnit {
    unit_id: &'static str,
    info: &'static str,
    trays: Vec<TestMaterialPatchTray>,
}

#[derive(Serialize)]
struct TestMaterialPatchTray {
    tray_id: &'static str,
    #[serde(rename = "type")]
    material_type: &'static str,
}

#[derive(Serialize)]
struct TestExternalSpool {}
