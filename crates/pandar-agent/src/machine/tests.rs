use std::{collections::BTreeMap, time::Duration};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{Mutex, Notify, mpsc};

use super::*;
use crate::AgentConfig;
use crate::machine::{
    file_transfer::{
        FakeMachineFileTransfer, FileTransferRequest, TransferProtectionMode::ProtectedData,
    },
    mqtt::{BAMBU_MQTT_QOS, BambuMqttTransport, FakeMqttTransport, PublishedMqttCommand},
    print::pick_remote_name,
    runtime::test_support::{TestRuntimeBambuMachineGateway, assert_locked_for_a_moment},
};

fn endpoint(serial: &str) -> BambuPrinterEndpoint {
    BambuPrinterEndpoint {
        host: "192.0.2.10".to_string(),
        serial: serial.to_string(),
        access_code: "12345678".to_string(),
        model: Some("A1 Mini".to_string()),
        name: Some(format!("printer-{serial}")),
    }
}

fn endpoint_without_model(serial: &str) -> BambuPrinterEndpoint {
    let mut endpoint = endpoint(serial);
    endpoint.model = None;
    endpoint
}

fn runtime_endpoint(serial: &str, name: &str, access_code: &str) -> BambuPrinterEndpoint {
    BambuPrinterEndpoint {
        host: "192.0.2.10".to_string(),
        serial: serial.to_string(),
        access_code: access_code.to_string(),
        model: Some("X1 Carbon".to_string()),
        name: Some(name.to_string()),
    }
}

fn get_version_report(model: &str) -> serde_json::Value {
    serde_json::to_value(TestGetVersionReport {
        info: TestGetVersionInfo {
            command: "get_version",
            module: [TestGetVersionModule {
                name: "ota",
                product_name: model,
            }],
        },
    })
    .unwrap()
}

fn runtime_state_report(state: &str) -> serde_json::Value {
    serde_json::to_value(TestRuntimeStateReport {
        print: TestRuntimePrintReport {
            state,
            ams: TestRuntimeAmsReport {
                ams: [TestRuntimeAmsUnit {
                    id: "0",
                    tray: [TestRuntimeAmsTray {
                        id: "0",
                        tray_type: "PLA",
                    }],
                }],
            },
        },
    })
    .unwrap()
}

#[derive(Debug, Serialize)]
struct TestGetVersionReport<'a> {
    info: TestGetVersionInfo<'a>,
}

#[derive(Debug, Serialize)]
struct TestGetVersionInfo<'a> {
    command: &'static str,
    module: [TestGetVersionModule<'a>; 1],
}

#[derive(Debug, Serialize)]
struct TestGetVersionModule<'a> {
    name: &'static str,
    product_name: &'a str,
}

#[derive(Debug, Serialize)]
struct TestRuntimeStateReport<'a> {
    print: TestRuntimePrintReport<'a>,
}

#[derive(Debug, Serialize)]
struct TestRuntimePrintReport<'a> {
    state: &'a str,
    ams: TestRuntimeAmsReport,
}

#[derive(Debug, Serialize)]
struct TestRuntimeAmsReport {
    ams: [TestRuntimeAmsUnit; 1],
}

#[derive(Debug, Serialize)]
struct TestRuntimeAmsUnit {
    id: &'static str,
    tray: [TestRuntimeAmsTray; 1],
}

#[derive(Debug, Serialize)]
struct TestRuntimeAmsTray {
    id: &'static str,
    tray_type: &'static str,
}

fn runtime_reports(model: &str, state: &str) -> [serde_json::Value; 2] {
    [get_version_report(model), runtime_state_report(state)]
}

fn operation_report(value: &Value) -> TestOperationReport {
    serde_json::from_value(value.clone()).unwrap()
}

#[derive(Debug, Deserialize)]
struct TestOperationReport {
    system: Option<TestOperationReportSection>,
    print: Option<TestOperationReportSection>,
}

#[derive(Debug, Deserialize)]
struct TestOperationReportSection {
    result: String,
}

fn runtime_transport(
    report_sets: impl IntoIterator<Item = (&'static str, &'static str)>,
) -> FakeMqttTransport {
    FakeMqttTransport::with_reports(
        report_sets
            .into_iter()
            .flat_map(|(model, state)| runtime_reports(model, state)),
    )
}

fn test_config() -> AgentConfig {
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

#[tokio::test]
async fn noop_refresh_printers_returns_no_snapshots() {
    let gateway = NoopMachineGateway;

    assert_eq!(gateway.refresh_printers().await.unwrap(), Vec::new());
}

#[tokio::test]
async fn configured_refresh_printers_refreshes_endpoints_sequentially() {
    let first = FakeMqttTransport::with_reports([
        get_version_report("P2S"),
        json!({"print": {"state": "READY"}}),
    ]);
    let second = FakeMqttTransport::with_reports([
        get_version_report("X1 Carbon"),
        json!({"state": "IDLE"}),
    ]);
    let first_endpoint = endpoint("SERIAL1");
    let second_endpoint = endpoint("SERIAL2");
    let gateway = ConfiguredBambuMachineGateway::new(
        vec![
            (first_endpoint.clone(), first.clone()),
            (second_endpoint.clone(), second.clone()),
        ],
        Duration::from_secs(1),
    );

    let snapshots = gateway
        .refresh_printers()
        .await
        .unwrap()
        .into_iter()
        .map(|result| result.snapshot)
        .collect::<Vec<_>>();

    assert_eq!(
        snapshots,
        vec![
            MachineSnapshot {
                serial: "SERIAL1".to_string(),
                host: Some("192.0.2.10".to_string()),
                access_code: Some("12345678".to_string()),
                name: "printer-SERIAL1".to_string(),
                model: Some("P2S".to_string()),
                state: "READY".to_string(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_light_on: None,
            },
            MachineSnapshot {
                serial: "SERIAL2".to_string(),
                host: Some("192.0.2.10".to_string()),
                access_code: Some("12345678".to_string()),
                name: "printer-SERIAL2".to_string(),
                model: Some("X1 Carbon".to_string()),
                state: "IDLE".to_string(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_light_on: None,
            },
        ]
    );
    assert_eq!(
        first.subscriptions().await,
        [format!("device/{}/report", first_endpoint.serial)]
    );
    assert_eq!(
        second.subscriptions().await,
        [format!("device/{}/report", second_endpoint.serial)]
    );
    let published = first.published_commands().await;
    let get_version_sequence_id = dynamic_section_sequence_id(&published[0].payload, "info");
    let pushall_sequence_id = dynamic_section_sequence_id(&published[1].payload, "pushing");
    assert_eq!(
        published,
        [
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: json!({"info": {"command": "get_version", "sequence_id": get_version_sequence_id}}),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: json!({"pushing": {"command": "pushall", "sequence_id": pushall_sequence_id, "version": 1, "push_target": 1}}),
                qos: BAMBU_MQTT_QOS,
            },
        ]
    );
}

#[tokio::test]
async fn configured_gateway_construction_uses_runtime_ftps_without_network_io() {
    let mqtt = FakeMqttTransport::default();
    let gateway = ConfiguredBambuMachineGateway::new(
        vec![(endpoint("SERIAL1"), mqtt)],
        Duration::from_secs(1),
    );

    assert_eq!(gateway.configured_printer_count(), 1);
}

#[tokio::test]
async fn configured_print_project_file_uploads_and_publishes_project_file() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let endpoint = endpoint("SERIAL1");
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint.clone(), mqtt.clone(), transfer.clone())],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .print_project_file("SERIAL1", &print_project_file(), b"abc".to_vec())
        .await
        .unwrap();

    assert_eq!(
        transfer.recorded_requests(),
        vec![(
            ProtectedData,
            FileTransferRequest::upload("plate.gcode.3mf", 3)
        )]
    );
    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({
                "print": {
                    "command": "project_file",
                    "sequence_id": sequence_id,
                    "param": "Metadata/plate_1.gcode",
                    "project_id": "0",
                    "profile_id": "0",
                    "task_id": "0",
                    "subtask_id": "0",
                    "subtask_name": "plate",
                    "url": "ftp://plate.gcode.3mf",
                    "file": "plate.gcode.3mf",
                    "md5": "900150983CD24FB0D6963F7D28E17F72",
                    "bed_type": "auto",
                    "bed_leveling": false,
                    "flow_cali": false,
                    "vibration_cali": false,
                    "layer_inspect": false,
                    "timelapse": true,
                    "use_ams": true,
                    "ams_mapping": [],
                    "ams_mapping2": [],
                    "auto_bed_leveling": 0,
                    "nozzle_offset_cali": 0,
                    "cfg": "0",
                    "extrude_cali_flag": 0
                }
            }),
            qos: 0,
        }]
    );
}

#[test]
fn print_project_remote_name_matches_studio_suffix_policy() {
    assert_eq!(pick_remote_name("Cube"), "Cube.gcode.3mf");
    assert_eq!(pick_remote_name("plate.3mf"), "plate.gcode.3mf");
    assert_eq!(pick_remote_name("plate.gcode.3mf"), "plate.gcode.3mf");
    assert_eq!(pick_remote_name("../bad/name.3mf"), "name.gcode.3mf");
}

#[tokio::test]
async fn configured_print_project_file_does_not_publish_when_upload_fails() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::with_failures(true, true);
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer.clone())],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    let err = gateway
        .print_project_file("SERIAL1", &print_project_file(), b"abc".to_vec())
        .await
        .unwrap_err();
    let message = format!("{err:#}");

    assert!(message.contains("upload print artifact to SERIAL1"));
    assert!(message.contains("fake protected data failure"));
    assert!(message.contains("fake clear data failure"));
    assert!(mqtt.published_commands().await.is_empty());
}

#[tokio::test]
async fn configured_print_project_file_unknown_serial_rejects_before_upload() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer.clone())],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    let err = gateway
        .print_project_file("UNKNOWN", &print_project_file(), b"abc".to_vec())
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("UNKNOWN"));
    assert!(transfer.recorded_requests().is_empty());
    assert!(mqtt.published_commands().await.is_empty());
}

#[tokio::test]
async fn configured_operate_printer_publishes_pause_to_request_topic() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint_without_model("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer("SERIAL1", PrinterOperation::Pause)
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {"command": "pause", "param": "", "sequence_id": sequence_id}}),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_select_extruder_publishes_reference_command() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint_without_model("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer("SERIAL1", PrinterOperation::SelectExtruder(1))
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {"command": "select_extruder", "extruder_index": 1, "sequence_id": sequence_id}}),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_toggle_light_sends_bambu_studio_light_nodes() {
    let mqtt = FakeMqttTransport::with_reports([json!({
        "print": {
            "lights_report": [{"node": "chamber_light", "mode": "on"}]
        }
    })]);
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint_without_model("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer("SERIAL1", PrinterOperation::ToggleLight)
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let pushall_sequence_id = dynamic_section_sequence_id(&published[0].payload, "pushing");
    let light_sequence_id = dynamic_section_sequence_id(&published[1].payload, "system");
    let light2_sequence_id = dynamic_section_sequence_id(&published[2].payload, "system");
    assert_eq!(
        published,
        vec![
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: json!({"pushing": {
                    "command": "pushall",
                    "sequence_id": pushall_sequence_id,
                    "version": 1,
                    "push_target": 1
                }}),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: json!({"system": {
                    "command": "ledctrl",
                    "led_node": "chamber_light",
                    "led_mode": "off",
                    "led_on_time": 500,
                    "led_off_time": 500,
                    "loop_times": 1,
                    "interval_time": 1000,
                    "sequence_id": light_sequence_id
                }}),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: json!({"system": {
                    "command": "ledctrl",
                    "led_node": "chamber_light2",
                    "led_mode": "off",
                    "led_on_time": 500,
                    "led_off_time": 500,
                    "loop_times": 1,
                    "interval_time": 1000,
                    "sequence_id": light2_sequence_id
                }}),
                qos: BAMBU_MQTT_QOS,
            },
        ]
    );
}

#[tokio::test]
async fn configured_operate_printer_toggle_light_matches_bambu_studio_light_nodes() {
    let mqtt = FakeMqttTransport::with_reports([json!({
        "print": {
            "lights_report": [
                {"node": "chamber_light", "mode": "off"},
                {"node": "chamber_light2", "mode": "off"}
            ]
        }
    })]);
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint_without_model("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer("SERIAL1", PrinterOperation::ToggleLight)
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let pushall_sequence_id = dynamic_section_sequence_id(&published[0].payload, "pushing");
    let light_sequence_id = dynamic_section_sequence_id(&published[1].payload, "system");
    let light2_sequence_id = dynamic_section_sequence_id(&published[2].payload, "system");
    assert_eq!(
        published,
        vec![
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: json!({"pushing": {
                    "command": "pushall",
                    "sequence_id": pushall_sequence_id,
                    "version": 1,
                    "push_target": 1
                }}),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: json!({"system": {
                    "command": "ledctrl",
                    "led_node": "chamber_light",
                    "led_mode": "on",
                    "led_on_time": 500,
                    "led_off_time": 500,
                    "loop_times": 1,
                    "interval_time": 1000,
                    "sequence_id": light_sequence_id
                }}),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: json!({"system": {
                    "command": "ledctrl",
                    "led_node": "chamber_light2",
                    "led_mode": "on",
                    "led_on_time": 500,
                    "led_off_time": 500,
                    "loop_times": 1,
                    "interval_time": 1000,
                    "sequence_id": light2_sequence_id
                }}),
                qos: BAMBU_MQTT_QOS,
            },
        ]
    );
}

#[tokio::test]
async fn configured_operate_printer_set_chamber_light_uses_requested_state() {
    let mqtt = FakeMqttTransport::with_reports([json!({
        "print": {
            "lights_report": [{"node": "chamber_light", "mode": "on"}]
        }
    })]);
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint_without_model("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer("SERIAL1", PrinterOperation::SetChamberLight(true))
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let pushall_sequence_id = dynamic_section_sequence_id(&published[0].payload, "pushing");
    let light_sequence_id = dynamic_section_sequence_id(&published[1].payload, "system");
    let light2_sequence_id = dynamic_section_sequence_id(&published[2].payload, "system");
    assert_eq!(
        published,
        vec![
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: json!({"pushing": {
                    "command": "pushall",
                    "sequence_id": pushall_sequence_id,
                    "version": 1,
                    "push_target": 1
                }}),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: json!({"system": {
                    "command": "ledctrl",
                    "led_node": "chamber_light",
                    "led_mode": "on",
                    "led_on_time": 500,
                    "led_off_time": 500,
                    "loop_times": 1,
                    "interval_time": 1000,
                    "sequence_id": light_sequence_id
                }}),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: json!({"system": {
                    "command": "ledctrl",
                    "led_node": "chamber_light2",
                    "led_mode": "on",
                    "led_on_time": 500,
                    "led_off_time": 500,
                    "loop_times": 1,
                    "interval_time": 1000,
                    "sequence_id": light2_sequence_id
                }}),
                qos: BAMBU_MQTT_QOS,
            },
        ]
    );
}

#[tokio::test]
async fn configured_operate_printer_light_returns_primary_success_when_light2_fails() {
    let mqtt = FakeMqttTransport::with_reports_and_operation_reports_failed_led_node(
        [json!({
            "print": {
                "lights_report": [{"node": "chamber_light", "mode": "off"}]
            }
        })],
        "chamber_light2",
    );
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint_without_model("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    let result = gateway
        .operate_printer("SERIAL1", PrinterOperation::SetChamberLight(true))
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let primary_sequence_id = dynamic_section_sequence_id(&published[1].payload, "system");
    let light2_sequence_id = dynamic_section_sequence_id(&published[2].payload, "system");
    assert_ne!(primary_sequence_id, light2_sequence_id);
    assert_eq!(
        result.sequence_id.as_deref(),
        Some(primary_sequence_id.as_str())
    );
    assert_eq!(
        operation_report(result.mqtt_report.as_ref().unwrap())
            .system
            .unwrap()
            .result,
        "success"
    );
    assert_eq!(result.error, None);
}

#[tokio::test]
async fn configured_operate_printer_returns_matching_mqtt_sequence_result() {
    let mqtt = FakeMqttTransport::with_operation_reports();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint_without_model("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    let result = gateway
        .operate_printer("SERIAL1", PrinterOperation::Pause)
        .await
        .unwrap();

    let sequence_id = dynamic_sequence_id(&mqtt.published_commands().await[0].payload);
    assert_eq!(result.sequence_id.as_deref(), Some(sequence_id.as_str()));
    assert_eq!(
        operation_report(result.mqtt_report.as_ref().unwrap())
            .print
            .unwrap()
            .result,
        "success"
    );
}

#[tokio::test]
async fn configured_operate_printer_print_speed_mode_4_publishes_to_request_topic() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer("SERIAL1", PrinterOperation::SetPrintSpeed(4))
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {"command": "print_speed", "param": "4", "sequence_id": sequence_id}}),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_home_publishes_bare_g28_for_axis_specific_request() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::Home {
                axes: vec![PrinterAxis::X, PrinterAxis::Z],
            },
        )
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {"command": "gcode_line", "param": "G28", "sequence_id": sequence_id}}),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_move_axes_publishes_relative_gcode_line() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::MoveAxes {
                x_mm: Some(10.0),
                y_mm: None,
                z_mm: Some(-0.5),
                feedrate_mm_per_min: Some(3000.0),
            },
        )
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {"command": "gcode_line", "param": "G91\nG0 X10 Z-0.5 F3000\nG90", "sequence_id": sequence_id}}),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_hotend_publishes_wait_gcode_line() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::SetHotendTemperature {
                temperature_celsius: 215,
                wait: true,
                extruder_id: None,
            },
        )
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {"command": "gcode_line", "param": "M109 S215", "sequence_id": sequence_id}}),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_targeted_hotend_publishes_reference_command() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::SetHotendTemperature {
                temperature_celsius: 220,
                wait: false,
                extruder_id: Some(1),
            },
        )
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {
                "command": "set_nozzle_temp",
                "extruder_index": 1,
                "target_temp": 220,
                "sequence_id": sequence_id
            }}),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

fn dynamic_sequence_id(payload: &Value) -> String {
    dynamic_section_sequence_id(payload, "print")
}

fn dynamic_section_sequence_id(payload: &Value, section: &str) -> String {
    let sections: BTreeMap<String, TestSequenceSection> =
        serde_json::from_value(payload.clone()).unwrap();
    let sequence_id = &sections.get(section).unwrap().sequence_id;
    assert_ne!(sequence_id, "0");
    assert!((20000..30000).contains(&sequence_id.parse::<u32>().unwrap()));
    sequence_id.to_string()
}

#[derive(Debug, Deserialize)]
struct TestSequenceSection {
    sequence_id: String,
}

#[tokio::test]
async fn configured_operate_printer_ams_load_publishes_change_filament_command() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::AmsLoadFilament {
                ams_id: 0,
                slot_id: 1,
                global_tray_id: Some(1),
                external_id: None,
                extruder_id: Some(0),
            },
        )
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {
                "command": "ams_change_filament",
                "sequence_id": sequence_id,
                "ams_id": 0,
                "slot_id": 1,
                "target": 1,
                "extruder_id": 0,
                "curr_temp": -1,
                "tar_temp": -1
            }}),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_bed_temperature_publishes_reference_gcode() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::SetBedTemperature {
                temperature_celsius: 75,
                wait: false,
            },
        )
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {"command": "gcode_line", "param": "M140 S75", "sequence_id": sequence_id}}),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_chamber_temperature_publishes_reference_gcode() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::SetChamberTemperature {
                temperature_celsius: 45,
                wait: false,
            },
        )
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {"command": "gcode_line", "param": "M141 S45", "sequence_id": sequence_id}}),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_ams_reread_rfid_publishes_get_rfid_command() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::AmsRereadRfid {
                ams_id: 0,
                slot_id: 1,
            },
        )
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {
                "command": "ams_get_rfid",
                "sequence_id": sequence_id,
                "ams_id": 0,
                "slot_id": 1
            }}),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_ams_reread_rfid_increments_sequence_id() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    for _ in 0..2 {
        gateway
            .operate_printer(
                "SERIAL1",
                PrinterOperation::AmsRereadRfid {
                    ams_id: 0,
                    slot_id: 1,
                },
            )
            .await
            .unwrap();
    }

    let published = mqtt.published_commands().await;
    let first = dynamic_sequence_id(&published[0].payload);
    let second = dynamic_sequence_id(&published[1].payload);

    assert_ne!(first, "0");
    assert_ne!(second, "0");
    assert_ne!(first, second);
}

#[tokio::test]
async fn configured_operate_printer_ams_unload_publishes_change_filament_unload_command() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::AmsUnloadFilament {
                ams_id: 0,
                slot_id: 1,
                global_tray_id: Some(1),
                external_id: None,
                extruder_id: None,
            },
        )
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {
                "command": "ams_change_filament",
                "sequence_id": sequence_id,
                "ams_id": 0,
                "slot_id": 255,
                "target": 255,
                "curr_temp": 210,
                "tar_temp": 210
            }}),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_unknown_serial_rejects_before_publish() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    let err = gateway
        .operate_printer("UNKNOWN", PrinterOperation::Pause)
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("UNKNOWN"));
    assert!(mqtt.published_commands().await.is_empty());
}

#[tokio::test]
async fn configured_print_project_file_rejects_unknown_flow_cali_before_upload() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let mut endpoint = endpoint("SERIAL1");
    endpoint.model = None;
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint, mqtt.clone(), transfer.clone())],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );
    let mut command = print_project_file();
    command.flow_cali = true;

    let err = gateway
        .print_project_file("SERIAL1", &command, b"abc".to_vec())
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("flow calibration"));
    assert!(transfer.recorded_requests().is_empty());
    assert!(mqtt.published_commands().await.is_empty());
}

#[tokio::test]
async fn configured_print_project_file_rejects_a1_flow_cali_before_upload() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer.clone())],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );
    let mut command = print_project_file();
    command.flow_cali = true;

    let err = gateway
        .print_project_file("SERIAL1", &command, b"abc".to_vec())
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("flow calibration"));
    assert!(transfer.recorded_requests().is_empty());
    assert!(mqtt.published_commands().await.is_empty());
}

fn print_project_file() -> PrintProjectFile {
    PrintProjectFile {
        job_id: "job-1".to_string(),
        artifact_id: "artifact-1".to_string(),
        printer_id: "printer-1".to_string(),
        serial_number: "SERIAL1".to_string(),
        filename: "plate.3mf".to_string(),
        storage_path: "tenant/artifact/plate.3mf".to_string(),
        artifact_download_path: "/api/v1/agents/agent-1/artifacts/artifact-1".to_string(),
        size_bytes: 3,
        plate_id: 1,
        use_ams: true,
        flow_cali: false,
        timelapse: true,
        ams_mapping_json: String::new(),
        ams_mapping2_json: String::new(),
        ams_mapping_info_json: String::new(),
    }
}

mod runtime {
    use super::*;

    #[tokio::test]
    async fn empty_runtime_gateway_refresh_printers_returns_empty() {
        let gateway = TestRuntimeBambuMachineGateway::new(
            Vec::<(
                BambuPrinterEndpoint,
                FakeMqttTransport,
                FakeMachineFileTransfer,
            )>::new(),
            FakeMachineFileTransfer::default(),
            Duration::from_secs(1),
        );

        assert_eq!(gateway.refresh_printers().await.unwrap(), Vec::new());
    }

    #[tokio::test]
    async fn successful_link_printer_installs_endpoint_for_later_refresh() {
        let gateway = TestRuntimeBambuMachineGateway::new(
            Vec::new(),
            FakeMachineFileTransfer::default(),
            Duration::from_secs(1),
        );
        gateway
            .push_command_transport(runtime_transport([
                ("X1 Carbon", "READY"),
                ("X1 Carbon", "IDLE"),
            ]))
            .await;
        let (sender, _) = mpsc::channel(1);

        let snapshot = gateway
            .link_printer(
                runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
                &test_config(),
                &sender,
            )
            .await
            .unwrap();

        assert_eq!(snapshot.state, "READY");
        assert_eq!(gateway.report_task_count("SERIAL1").await, 1);
        assert_eq!(
            gateway
                .refresh_printers()
                .await
                .unwrap()
                .into_iter()
                .map(|result| result.snapshot)
                .collect::<Vec<_>>(),
            vec![MachineSnapshot {
                serial: "SERIAL1".to_string(),
                host: Some("192.0.2.10".to_string()),
                access_code: Some("ACCESS-1".to_string()),
                name: "office".to_string(),
                model: Some("X1 Carbon".to_string()),
                state: "IDLE".to_string(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_light_on: None,
            }]
        );
    }

    #[tokio::test]
    async fn same_serial_replacement_after_validation_success_leaves_one_report_task() {
        let gateway = TestRuntimeBambuMachineGateway::new(
            Vec::new(),
            FakeMachineFileTransfer::default(),
            Duration::from_secs(1),
        );
        gateway
            .push_command_transport(runtime_transport([("X1 Carbon", "READY")]))
            .await;
        gateway
            .push_command_transport(runtime_transport([("P2S", "RUNNING"), ("P2S", "PAUSED")]))
            .await;
        let (sender, _) = mpsc::channel(1);

        gateway
            .link_printer(
                runtime_endpoint("SERIAL1", "old office", "ACCESS-1"),
                &test_config(),
                &sender,
            )
            .await
            .unwrap();
        gateway
            .link_printer(
                runtime_endpoint("SERIAL1", "new office", "ACCESS-2"),
                &test_config(),
                &sender,
            )
            .await
            .unwrap();

        assert_eq!(gateway.report_task_count("SERIAL1").await, 1);
        assert_eq!(
            gateway
                .refresh_printers()
                .await
                .unwrap()
                .into_iter()
                .map(|result| result.snapshot)
                .collect::<Vec<_>>(),
            vec![MachineSnapshot {
                serial: "SERIAL1".to_string(),
                host: Some("192.0.2.10".to_string()),
                access_code: Some("ACCESS-2".to_string()),
                name: "new office".to_string(),
                model: Some("P2S".to_string()),
                state: "PAUSED".to_string(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_light_on: None,
            }]
        );
    }

    #[tokio::test]
    async fn same_serial_replacement_after_validation_failure_leaves_previous_endpoint_active() {
        let gateway = TestRuntimeBambuMachineGateway::new(
            Vec::new(),
            FakeMachineFileTransfer::default(),
            Duration::from_secs(1),
        );
        gateway
            .push_command_transport(runtime_transport([
                ("X1 Carbon", "READY"),
                ("X1 Carbon", "IDLE"),
            ]))
            .await;
        gateway
            .push_command_transport(FakeMqttTransport::with_timeout())
            .await;
        let (sender, _) = mpsc::channel(1);

        gateway
            .link_printer(
                runtime_endpoint("SERIAL1", "old office", "ACCESS-1"),
                &test_config(),
                &sender,
            )
            .await
            .unwrap();
        let err = gateway
            .link_printer(
                runtime_endpoint("SERIAL1", "new office", "ACCESS-2"),
                &test_config(),
                &sender,
            )
            .await
            .unwrap_err();

        assert!(format!("{err:#}").contains("validate runtime printer SERIAL1"));
        assert_eq!(gateway.report_task_count("SERIAL1").await, 1);
        assert_eq!(
            gateway
                .refresh_printers()
                .await
                .unwrap()
                .into_iter()
                .map(|result| result.snapshot)
                .collect::<Vec<_>>(),
            vec![MachineSnapshot {
                serial: "SERIAL1".to_string(),
                host: Some("192.0.2.10".to_string()),
                access_code: Some("ACCESS-1".to_string()),
                name: "old office".to_string(),
                model: Some("X1 Carbon".to_string()),
                state: "IDLE".to_string(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_light_on: None,
            }]
        );
    }

    #[tokio::test]
    async fn concurrent_same_serial_link_printer_calls_are_serialized() {
        let gateway = std::sync::Arc::new(TestRuntimeBambuMachineGateway::new(
            Vec::new(),
            FakeMachineFileTransfer::default(),
            Duration::from_secs(1),
        ));
        let paused = PausedMqttTransport::new();
        gateway.push_command_transport(paused.clone()).await;
        gateway
            .push_command_transport(PausedMqttTransport::ready("P2S", "IDLE"))
            .await;
        let (sender, _) = mpsc::channel(1);
        let config = test_config();

        let first_gateway = std::sync::Arc::clone(&gateway);
        let first_sender = sender.clone();
        let first_config = config.clone();
        let first = tokio::spawn(async move {
            first_gateway
                .link_printer(
                    runtime_endpoint("SERIAL1", "first", "ACCESS-1"),
                    &first_config,
                    &first_sender,
                )
                .await
        });
        paused.wait_until_blocked().await;
        assert_locked_for_a_moment(&gateway).await.unwrap();

        let second_gateway = std::sync::Arc::clone(&gateway);
        let second_sender = sender.clone();
        let second_config = config.clone();
        let second = tokio::spawn(async move {
            second_gateway
                .link_printer(
                    runtime_endpoint("SERIAL1", "second", "ACCESS-2"),
                    &second_config,
                    &second_sender,
                )
                .await
        });
        tokio::task::yield_now().await;
        assert!(!second.is_finished());

        paused.release();
        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
        assert_eq!(gateway.report_task_count("SERIAL1").await, 1);
    }

    #[tokio::test]
    async fn runtime_install_keeps_lock_until_report_task_replacement_finishes() {
        let gateway = std::sync::Arc::new(TestRuntimeBambuMachineGateway::new(
            Vec::new(),
            FakeMachineFileTransfer::default(),
            Duration::from_secs(1),
        ));
        gateway
            .push_command_transport(runtime_transport([("X1 Carbon", "READY")]))
            .await;
        let pause = gateway.pause_report_task_replacement().await;
        let (sender, _) = mpsc::channel(1);
        let config = test_config();

        let link_gateway = std::sync::Arc::clone(&gateway);
        let link_sender = sender.clone();
        let link_config = config.clone();
        let link = tokio::spawn(async move {
            link_gateway
                .link_printer(
                    runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
                    &link_config,
                    &link_sender,
                )
                .await
        });
        pause.wait_until_blocked().await;

        assert_locked_for_a_moment(&gateway).await.unwrap();

        pause.release();
        link.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn report_forwarding_preparation_failure_leaves_previous_endpoint_active() {
        let gateway = TestRuntimeBambuMachineGateway::new(
            Vec::new(),
            FakeMachineFileTransfer::default(),
            Duration::from_secs(1),
        );
        gateway
            .push_command_transport(runtime_transport([
                ("X1 Carbon", "READY"),
                ("X1 Carbon", "IDLE"),
            ]))
            .await;
        gateway
            .push_command_transport(runtime_transport([("P2S", "RUNNING")]))
            .await;
        let (sender, _) = mpsc::channel(1);

        gateway
            .link_printer(
                runtime_endpoint("SERIAL1", "old office", "ACCESS-1"),
                &test_config(),
                &sender,
            )
            .await
            .unwrap();
        gateway
            .push_report_preparation_error(anyhow::anyhow!("prepare report transport failed"))
            .await;
        let err = gateway
            .link_printer(
                runtime_endpoint("SERIAL1", "new office", "ACCESS-2"),
                &test_config(),
                &sender,
            )
            .await
            .unwrap_err();

        assert!(format!("{err:#}").contains("prepare report transport failed"));
        assert_eq!(gateway.report_task_count("SERIAL1").await, 1);
        assert_eq!(
            gateway
                .refresh_printers()
                .await
                .unwrap()
                .into_iter()
                .map(|result| result.snapshot)
                .collect::<Vec<_>>(),
            vec![MachineSnapshot {
                serial: "SERIAL1".to_string(),
                host: Some("192.0.2.10".to_string()),
                access_code: Some("ACCESS-1".to_string()),
                name: "old office".to_string(),
                model: Some("X1 Carbon".to_string()),
                state: "IDLE".to_string(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_light_on: None,
            }]
        );
    }

    #[derive(Clone)]
    struct PausedMqttTransport {
        state: std::sync::Arc<PausedMqttTransportState>,
    }

    struct PausedMqttTransportState {
        blocked: Notify,
        release: Notify,
        reports: Mutex<Vec<serde_json::Value>>,
        pause_first_report: bool,
    }

    impl PausedMqttTransport {
        fn new() -> Self {
            Self {
                state: std::sync::Arc::new(PausedMqttTransportState {
                    blocked: Notify::new(),
                    release: Notify::new(),
                    reports: Mutex::new(vec![
                        get_version_report("X1 Carbon"),
                        runtime_state_report("READY"),
                    ]),
                    pause_first_report: true,
                }),
            }
        }

        fn ready(model: &str, state: &str) -> Self {
            Self {
                state: std::sync::Arc::new(PausedMqttTransportState {
                    blocked: Notify::new(),
                    release: Notify::new(),
                    reports: Mutex::new(vec![
                        get_version_report(model),
                        runtime_state_report(state),
                    ]),
                    pause_first_report: false,
                }),
            }
        }

        async fn wait_until_blocked(&self) {
            self.state.blocked.notified().await;
        }

        fn release(&self) {
            self.state.release.notify_waiters();
        }
    }

    #[async_trait]
    impl BambuMqttTransport for PausedMqttTransport {
        async fn subscribe(&self, _topic: &str) -> anyhow::Result<()> {
            Ok(())
        }

        async fn publish(&self, _command: PublishedMqttCommand) -> anyhow::Result<()> {
            Ok(())
        }

        async fn next_report(&self, _timeout: Duration) -> anyhow::Result<serde_json::Value> {
            if self.state.pause_first_report {
                let mut reports = self.state.reports.lock().await;
                if reports.len() == 2 {
                    self.state.blocked.notify_waiters();
                    drop(reports);
                    self.state.release.notified().await;
                    reports = self.state.reports.lock().await;
                }
                return Ok(reports.remove(0));
            }
            Ok(self.state.reports.lock().await.remove(0))
        }
    }
}
