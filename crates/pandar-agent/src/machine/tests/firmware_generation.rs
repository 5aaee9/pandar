use std::time::Duration;

use pandar_core::{PrinterFirmwareModule, PrinterFirmwareStatus, PrinterUpgradeState};
use tokio::sync::{mpsc, oneshot};

use crate::{
    machine::{
        FirmwareModulesObservation, FirmwareObservationCache, FirmwareReportContext,
        FirmwareStatusObservation, RuntimeReportContext,
        firmware_event_pause::{self, FirmwareEventKind},
    },
    protocol::agent::v1::{AgentEvent, agent_event},
};

use super::{endpoint, test_config};

mod generation;
mod observations;
mod ordering;
mod runtime_modules;

fn assert_invalidated(event: AgentEvent, generation: u64) {
    let agent_event::Event::PrinterFirmwareInvalidated(event) = event.event.unwrap() else {
        panic!("expected firmware invalidation first");
    };
    assert_eq!(event.serial, "SERIAL1");
    assert_eq!(event.generation, generation);
}

fn assert_mqtt_offline(event: AgentEvent) {
    let agent_event::Event::PrinterSnapshot(snapshot) = event.event.unwrap() else {
        panic!("expected MQTT offline snapshot");
    };
    assert_eq!(snapshot.state, "offline");
    assert!(!snapshot.telemetry_authoritative);
}

async fn next_firmware_event(receiver: &mut mpsc::Receiver<AgentEvent>) -> AgentEvent {
    loop {
        let event = receiver.recv().await.unwrap();
        if matches!(
            event.event,
            Some(
                agent_event::Event::PrinterFirmwareModulesSnapshot(_)
                    | agent_event::Event::PrinterFirmwareStatusSnapshot(_)
                    | agent_event::Event::PrinterFirmwareInvalidated(_)
            )
        ) {
            return event;
        }
    }
}

fn module(version: &str) -> PrinterFirmwareModule {
    PrinterFirmwareModule {
        name: "ota".to_owned(),
        software_version: Some(version.to_owned()),
        software_new_version: None,
        new_version: None,
        visible: None,
        product_name: Some("X1".to_owned()),
        serial_number: None,
        hardware_version: None,
        firmware_flag: None,
    }
}

fn version_report(version: &str) -> serde_json::Value {
    serde_json::json!({
        "info": {
            "command": "get_version",
            "module": [{
                "name": "ota",
                "product_name": "X1",
                "sw_ver": version
            }]
        }
    })
}

fn status(value: &str) -> PrinterFirmwareStatus {
    PrinterFirmwareStatus {
        upgrade_state: Some(PrinterUpgradeState {
            status: Some(value.to_owned()),
            progress: None,
            message: None,
            module: None,
            error_code: None,
            new_version_state: None,
            consistency_request: None,
            force_upgrade: None,
            display_state: None,
            ota_new_version_number: None,
            ams_new_version_number: None,
            ahb_new_version_number: None,
            new_versions: None,
            ams_firmware: None,
        }),
        cfg: None,
    }
}
