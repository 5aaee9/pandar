use std::time::Duration;

use async_trait::async_trait;
use pandar_core::BambuDeviceFeatures;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Mutex, Notify, mpsc};

use super::*;
use crate::AgentConfig;
use crate::machine::{
    file_transfer::{
        FakeMachineFileTransfer, FileTransferRequest, TransferProtectionMode::ProtectedData,
    },
    mqtt::{
        BAMBU_MQTT_QOS, BAMBU_MQTT_RETAIN, BambuMqttTransport, FakeMqttTransport, PrintErrorAction,
        PublishedMqttCommand,
    },
    print::pick_remote_name,
    runtime::test_support::{TestRuntimeBambuMachineGateway, assert_locked_for_a_moment},
};
use crate::protocol::agent::v1::{HubCommand, RefreshPrinters, agent_event, hub_command};

mod axis_controls;
mod fixtures;
mod print_error;

use fixtures::*;

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
            fun: None,
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

fn runtime_feature_report(state: &str, fun: &'static str) -> serde_json::Value {
    serde_json::to_value(TestRuntimeStateReport {
        print: TestRuntimePrintReport {
            state,
            fun: Some(fun),
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

fn runtime_fun_only_report(fun: &str) -> serde_json::Value {
    serde_json::to_value(TestRuntimeFunOnlyReport {
        print: TestRuntimeFunOnly { fun },
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
    #[serde(skip_serializing_if = "Option::is_none")]
    fun: Option<&'static str>,
    ams: TestRuntimeAmsReport,
}

#[derive(Debug, Serialize)]
struct TestRuntimeFunOnlyReport<'a> {
    print: TestRuntimeFunOnly<'a>,
}

#[derive(Debug, Serialize)]
struct TestRuntimeFunOnly<'a> {
    fun: &'a str,
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

fn operation_report(value: impl Serialize) -> TestOperationReport {
    let value = serde_json::to_value(value).unwrap();
    serde::Deserialize::deserialize(value).unwrap()
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
    let first =
        FakeMqttTransport::with_reports([get_version_report("P2S"), print_state_report("READY")]);
    let second = FakeMqttTransport::with_reports([
        get_version_report("X1 Carbon"),
        root_state_report("IDLE"),
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
                device_features: None,
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
                device_features: None,
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
                payload: expected_get_version_payload(&get_version_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_pushall_payload(&pushall_sequence_id),
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
    let submission_id = dynamic_project_file_submission_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: expected_project_file_payload(&sequence_id, &submission_id),
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
            payload: expected_print_command_payload("pause", "", &sequence_id),
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
            payload: expected_select_extruder_payload(1, &sequence_id),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_toggle_light_sends_bambu_studio_light_nodes() {
    let mqtt = FakeMqttTransport::with_reports([lights_report(&[("chamber_light", "on")])]);
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
                payload: expected_pushall_payload(&pushall_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_light_payload("chamber_light", "off", &light_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_light_payload("chamber_light2", "off", &light2_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
        ]
    );
}

#[tokio::test]
async fn configured_operate_printer_toggle_light_matches_bambu_studio_light_nodes() {
    let mqtt = FakeMqttTransport::with_reports([lights_report(&[
        ("chamber_light", "off"),
        ("chamber_light2", "off"),
    ])]);
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
                payload: expected_pushall_payload(&pushall_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_light_payload("chamber_light", "on", &light_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_light_payload("chamber_light2", "on", &light2_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
        ]
    );
}

#[tokio::test]
async fn configured_operate_printer_set_chamber_light_uses_requested_state() {
    let mqtt = FakeMqttTransport::with_reports([lights_report(&[("chamber_light", "on")])]);
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
                payload: expected_pushall_payload(&pushall_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_light_payload("chamber_light", "on", &light_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_light_payload("chamber_light2", "on", &light2_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
        ]
    );
}

#[tokio::test]
async fn configured_operate_printer_light_returns_primary_success_when_light2_fails() {
    let mqtt = FakeMqttTransport::with_reports_and_operation_reports_failed_led_node(
        [lights_report(&[("chamber_light", "off")])],
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
            payload: expected_print_command_payload("print_speed", "4", &sequence_id),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_gcode_line_preserves_exact_param() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );
    let param = "M106 P1 S127 \r\n; keep  \n\n";

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::GcodeLine {
                param: param.to_owned(),
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
            payload: expected_print_command_payload("gcode_line", param, &sequence_id),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_home_preserves_axis_specific_request() {
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
                required_feature: None,
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
            payload: expected_print_command_payload("gcode_line", "G28 X Z", &sequence_id),
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
                required_feature: None,
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
            payload: expected_print_command_payload(
                "gcode_line",
                "M211 S\nM211 X1 Y1 Z1\nM1002 push_ref_mode\nG91\nG1 X10 Z-0.5 F3000\nM1002 pop_ref_mode\nM211 R",
                &sequence_id,
            ),
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
            payload: expected_print_command_payload("gcode_line", "M109 S215", &sequence_id),
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
            payload: expected_targeted_hotend_payload(1, 220, &sequence_id),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

fn dynamic_sequence_id(payload: &Value) -> String {
    dynamic_section_sequence_id(payload, "print")
}

fn dynamic_project_file_submission_id(payload: &Value) -> String {
    let envelope: TestProjectFileEnvelope = decode_payload(payload);
    let submission_id = envelope.print.project_id;
    assert_ne!(submission_id, "0");
    assert!((1..=2_147_483_647).contains(&submission_id.parse::<u32>().unwrap()));
    assert_eq!(envelope.print.task_id, submission_id);
    assert_eq!(envelope.print.subtask_id, submission_id);
    submission_id
}

fn dynamic_section_sequence_id(payload: &Value, section: &str) -> String {
    let envelope: TestSequenceEnvelope = decode_payload(payload);
    let sequence_id = &envelope.section(section).sequence_id;
    assert_ne!(sequence_id, "0");
    assert!((20000..30000).contains(&sequence_id.parse::<u32>().unwrap()));
    sequence_id.to_string()
}

fn decode_payload<T>(payload: &Value) -> T
where
    T: for<'de> Deserialize<'de>,
{
    T::deserialize(payload).unwrap()
}

#[derive(Debug, Deserialize)]
struct TestSequenceEnvelope {
    info: Option<TestSequenceSection>,
    pushing: Option<TestSequenceSection>,
    print: Option<TestSequenceSection>,
    system: Option<TestSequenceSection>,
}

impl TestSequenceEnvelope {
    fn section(&self, section: &str) -> &TestSequenceSection {
        match section {
            "info" => self.info.as_ref(),
            "pushing" => self.pushing.as_ref(),
            "print" => self.print.as_ref(),
            "system" => self.system.as_ref(),
            _ => None,
        }
        .unwrap()
    }
}

#[derive(Debug, Deserialize)]
struct TestSequenceSection {
    sequence_id: String,
}

#[derive(Debug, Deserialize)]
struct TestProjectFileEnvelope {
    print: TestProjectFileSection,
}

#[derive(Debug, Deserialize)]
struct TestProjectFileSection {
    project_id: String,
    task_id: String,
    subtask_id: String,
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
            payload: expected_ams_change_filament_payload(0, 1, 1, Some(0), -1, -1, &sequence_id),
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
            payload: expected_print_command_payload("gcode_line", "M140 S75", &sequence_id),
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
            payload: expected_print_command_payload("gcode_line", "M141 S45", &sequence_id),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_waiting_chamber_temperature_preserves_reference_gcode() {
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
                wait: true,
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
            payload: expected_print_command_payload(
                "gcode_line",
                "M106 P2 S255\nM191 S45\nM106 P2 S0",
                &sequence_id,
            ),
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
            payload: expected_ams_get_rfid_payload(0, 1, &sequence_id),
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
            payload: expected_ams_change_filament_payload(
                0,
                255,
                255,
                None,
                210,
                210,
                &sequence_id
            ),
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

    const DEVICE_FEATURE_HIGH_BITS: u64 = 0x8000_0041_0000_0020;

    #[tokio::test]
    async fn device_features_session_startup_precedes_queued_command_and_refreshes_zero() {
        let transport = FakeMqttTransport::with_reports([
            runtime_feature_report("RUNNING", "8000004100000020"),
            get_version_report("X1 Carbon"),
            runtime_feature_report("READY", "0"),
        ]);
        let transfer = FakeMachineFileTransfer::default();
        let gateway = std::sync::Arc::new(TestRuntimeBambuMachineGateway::new(
            vec![(
                runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
                transport.clone(),
                transfer.clone(),
            )],
            transfer,
            Duration::from_secs(1),
        ));
        let cache = gateway.device_feature_cache();
        cache
            .update(
                "SERIAL1",
                BambuDeviceFeatures::from_bits(DEVICE_FEATURE_HIGH_BITS),
            )
            .await;
        let config = test_config();
        let (sender, mut events) = mpsc::channel(16);
        sender.send(crate::hello_event(&config)).await.unwrap();
        let (commands_sender, commands_receiver) = mpsc::channel(1);
        commands_sender
            .send(Ok(HubCommand {
                command_id: "refresh-after-features".to_owned(),
                command: Some(hub_command::Command::RefreshPrinters(RefreshPrinters {})),
            }))
            .await
            .unwrap();
        let (command_release, released) = tokio::sync::oneshot::channel();

        let task = tokio::spawn({
            let gateway = std::sync::Arc::clone(&gateway);
            let config = config.clone();
            async move {
                gateway.prepare_session(&config, &sender).await?;
                released.await.expect("release queued Hub command");
                crate::handle_command_stream_with_gateway(
                    &config,
                    gateway.as_ref(),
                    &sender,
                    tokio_stream::wrappers::ReceiverStream::new(commands_receiver),
                )
                .await
            }
        });

        assert!(matches!(
            events.recv().await.unwrap().event,
            Some(agent_event::Event::Hello(_))
        ));
        assert_eq!(feature_event_bits(events.recv().await.unwrap()), None);
        assert_eq!(
            feature_event_bits(events.recv().await.unwrap()),
            Some(DEVICE_FEATURE_HIGH_BITS)
        );
        assert_eq!(
            cache.get("SERIAL1").await.unwrap().bits(),
            DEVICE_FEATURE_HIGH_BITS
        );
        command_release.send(()).unwrap();
        assert!(matches!(
            events.recv().await.unwrap().event,
            Some(agent_event::Event::CommandAck(_))
        ));
        let full_snapshot = events.recv().await.unwrap();
        let Some(agent_event::Event::PrinterSnapshot(full_snapshot)) = full_snapshot.event else {
            panic!("expected refreshed full printer snapshot");
        };
        assert_eq!(
            full_snapshot.device_features.unwrap().bambu_fun_bits,
            0,
            "valid zero must overwrite the prior nonzero value"
        );
        assert!(matches!(
            events.recv().await.unwrap().event,
            Some(agent_event::Event::PrinterMaterialsSnapshot(_))
        ));
        assert!(matches!(
            events.recv().await.unwrap().event,
            Some(agent_event::Event::CommandResult(result)) if result.success
        ));
        assert_eq!(cache.get("SERIAL1").await.unwrap().bits(), 0);

        let published = transport.published_commands().await;
        assert_eq!(published[0].payload["pushing"]["command"], "pushall");
        assert_eq!(published[1].payload["info"]["command"], "get_version");
        assert_eq!(published[2].payload["pushing"]["command"], "pushall");
        task.abort();
    }

    #[tokio::test]
    async fn device_features_session_startup_aborts_stale_report_cache_writer() {
        let transport = FakeMqttTransport::with_reports([runtime_fun_only_report("0")]);
        let transfer = FakeMachineFileTransfer::default();
        let gateway = std::sync::Arc::new(TestRuntimeBambuMachineGateway::new(
            vec![(
                runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
                transport,
                transfer.clone(),
            )],
            transfer,
            Duration::from_secs(1),
        ));
        let replacement_pause = gateway.pause_report_task_replacement().await;
        let release = std::sync::Arc::new(Notify::new());
        let cache = gateway.device_feature_cache();
        let stale_finished = install_stale_report_cache_write(
            &gateway.report_tasks,
            cache.clone(),
            "SERIAL1",
            BambuDeviceFeatures::from_bits(DEVICE_FEATURE_HIGH_BITS),
            std::sync::Arc::clone(&release),
        )
        .await;
        let (sender, mut events) = mpsc::channel(4);

        let prepare = tokio::spawn({
            let gateway = std::sync::Arc::clone(&gateway);
            async move { gateway.prepare_session(&test_config(), &sender).await }
        });
        replacement_pause.wait_until_blocked().await;
        assert_eq!(feature_event_bits(events.recv().await.unwrap()), None);
        assert_eq!(feature_event_bits(events.recv().await.unwrap()), Some(0));
        release.notify_waiters();
        tokio::task::yield_now().await;
        replacement_pause.release();
        prepare.await.unwrap().unwrap();

        assert!(stale_finished.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(cache.get("SERIAL1").await.unwrap().bits(), 0);
    }

    #[tokio::test]
    async fn device_features_report_reconnect_invalidates_before_accepting_new_value() {
        let transport =
            FakeMqttTransport::with_receive_failure_then_reports([runtime_fun_only_report(
                "8000004100000020",
            )]);
        let cache = crate::machine::DeviceFeatureCache::default();
        cache
            .update("SERIAL1", BambuDeviceFeatures::from_bits(0x40))
            .await;
        let (sender, mut events) = mpsc::channel(4);
        let task = tokio::spawn(crate::machine::runtime::forward_print_reports_with_retry(
            test_config(),
            transport.clone(),
            runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
            Duration::from_secs(1),
            sender,
            Duration::from_millis(1),
            cache.clone(),
        ));

        assert_eq!(feature_event_bits(events.recv().await.unwrap()), None);
        assert_eq!(
            feature_event_bits(events.recv().await.unwrap()),
            Some(DEVICE_FEATURE_HIGH_BITS)
        );
        assert_eq!(
            cache.get("SERIAL1").await.unwrap().bits(),
            DEVICE_FEATURE_HIGH_BITS
        );
        assert_eq!(transport.subscribe_attempts().await, 2);
        let published = transport.published_commands().await;
        assert_eq!(published.len(), 2);
        assert_eq!(published[0].payload["pushing"]["command"], "pushall");
        assert_eq!(published[1].payload["pushing"]["command"], "pushall");
        task.abort();
    }

    #[tokio::test]
    async fn device_features_report_failure_invalidates_before_retry_delay() {
        let transport = FakeMqttTransport::with_receive_failure_then_reports([]);
        let cache = crate::machine::DeviceFeatureCache::default();
        cache
            .update(
                "SERIAL1",
                BambuDeviceFeatures::from_bits(DEVICE_FEATURE_HIGH_BITS),
            )
            .await;
        let (sender, mut events) = mpsc::channel(2);
        let task = tokio::spawn(crate::machine::runtime::forward_print_reports_with_retry(
            test_config(),
            transport.clone(),
            runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
            Duration::from_secs(1),
            sender,
            Duration::from_secs(30),
            cache.clone(),
        ));

        let event = tokio::time::timeout(Duration::from_millis(100), events.recv())
            .await
            .expect("failure should invalidate before the retry delay")
            .unwrap();
        assert_eq!(feature_event_bits(event), None);
        assert_eq!(cache.get("SERIAL1").await, None);
        assert_eq!(transport.subscribe_attempts().await, 1);
        assert_eq!(transport.published_commands().await.len(), 1);
        task.abort();
    }

    #[tokio::test]
    async fn device_features_idle_timeout_does_not_invalidate_or_reprobe() {
        let transport = FakeMqttTransport::with_timeout();
        let cache = crate::machine::DeviceFeatureCache::default();
        cache
            .update(
                "SERIAL1",
                BambuDeviceFeatures::from_bits(DEVICE_FEATURE_HIGH_BITS),
            )
            .await;
        let (sender, mut events) = mpsc::channel(2);
        let task = tokio::spawn(crate::machine::runtime::forward_print_reports_with_retry(
            test_config(),
            transport.clone(),
            runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
            Duration::from_millis(1),
            sender,
            Duration::from_millis(1),
            cache.clone(),
        ));

        tokio::time::timeout(Duration::from_secs(1), async {
            while transport.published_commands().await.is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }

        assert_eq!(transport.subscribe_attempts().await, 1);
        assert_eq!(transport.published_commands().await.len(), 1);
        assert_eq!(
            cache.get("SERIAL1").await.unwrap().bits(),
            DEVICE_FEATURE_HIGH_BITS
        );
        assert!(events.try_recv().is_err());
        task.abort();
    }

    #[tokio::test]
    async fn device_features_endpoint_replacement_invalidates_before_new_snapshot() {
        let transfer = FakeMachineFileTransfer::default();
        let gateway = std::sync::Arc::new(TestRuntimeBambuMachineGateway::new(
            vec![(
                runtime_endpoint("SERIAL1", "old office", "ACCESS-1"),
                PausedMqttTransport::ready("X1 Carbon", "READY"),
                transfer.clone(),
            )],
            transfer,
            Duration::from_secs(1),
        ));
        let replacement = PausedMqttTransport::new_with_feature("0");
        gateway.push_command_transport(replacement.clone()).await;
        let replacement_pause = gateway.pause_report_task_replacement().await;
        let cache = gateway.device_feature_cache();
        cache
            .update(
                "SERIAL1",
                BambuDeviceFeatures::from_bits(DEVICE_FEATURE_HIGH_BITS),
            )
            .await;
        let stale_release = std::sync::Arc::new(Notify::new());
        let stale_finished = install_stale_report_cache_write(
            &gateway.report_tasks,
            cache.clone(),
            "SERIAL1",
            BambuDeviceFeatures::from_bits(DEVICE_FEATURE_HIGH_BITS),
            std::sync::Arc::clone(&stale_release),
        )
        .await;
        let (sender, mut events) = mpsc::channel(4);
        let config = test_config();
        let link = tokio::spawn({
            let gateway = std::sync::Arc::clone(&gateway);
            async move {
                gateway
                    .link_printer(
                        runtime_endpoint("SERIAL1", "new office", "ACCESS-2"),
                        &config,
                        &sender,
                    )
                    .await
            }
        });

        replacement.wait_until_blocked().await;
        assert_eq!(
            cache.get("SERIAL1").await.unwrap().bits(),
            DEVICE_FEATURE_HIGH_BITS
        );
        assert!(events.try_recv().is_err());

        replacement.release();
        replacement_pause.wait_until_blocked().await;
        stale_release.notify_waiters();
        tokio::task::yield_now().await;
        replacement_pause.release();
        let snapshot = link.await.unwrap().unwrap();
        assert_eq!(feature_event_bits(events.recv().await.unwrap()), None);
        assert!(stale_finished.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(snapshot.device_features.unwrap().bits(), 0);
        assert_eq!(cache.get("SERIAL1").await.unwrap().bits(), 0);
    }

    #[tokio::test]
    async fn device_features_invalid_refresh_keeps_cached_value() {
        let transport = FakeMqttTransport::with_reports([
            get_version_report("X1 Carbon"),
            runtime_feature_report("RUNNING", "not-hex"),
        ]);
        let transfer = FakeMachineFileTransfer::default();
        let gateway = TestRuntimeBambuMachineGateway::new(
            vec![(
                runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
                transport,
                transfer.clone(),
            )],
            transfer,
            Duration::from_secs(1),
        );
        let cache = gateway.device_feature_cache();
        cache
            .update(
                "SERIAL1",
                BambuDeviceFeatures::from_bits(DEVICE_FEATURE_HIGH_BITS),
            )
            .await;

        let snapshot = gateway.refresh_printers().await.unwrap().remove(0).snapshot;

        assert_eq!(snapshot.state, "RUNNING");
        assert_eq!(snapshot.device_features, None);
        assert_eq!(
            cache.get("SERIAL1").await.unwrap().bits(),
            DEVICE_FEATURE_HIGH_BITS
        );
    }

    fn feature_event_bits(event: crate::protocol::agent::v1::AgentEvent) -> Option<u64> {
        let Some(agent_event::Event::PrinterDeviceFeaturesSnapshot(snapshot)) = event.event else {
            panic!("expected printer device features event, got {event:?}");
        };
        snapshot
            .device_features
            .map(|features| features.bambu_fun_bits)
    }

    struct StaleReportTaskFinished(std::sync::Arc<std::sync::atomic::AtomicBool>);

    impl Drop for StaleReportTaskFinished {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    async fn install_stale_report_cache_write(
        report_tasks: &tokio::sync::Mutex<
            std::collections::HashMap<String, tokio::task::JoinHandle<()>>,
        >,
        cache: crate::machine::DeviceFeatureCache,
        serial: &str,
        value: BambuDeviceFeatures,
        release: std::sync::Arc<Notify>,
    ) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        let serial = serial.to_owned();
        let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let task_finished = std::sync::Arc::clone(&finished);
        let (started, running) = tokio::sync::oneshot::channel();
        report_tasks.lock().await.insert(
            serial.clone(),
            tokio::spawn(async move {
                let _finished = StaleReportTaskFinished(task_finished);
                started.send(()).unwrap();
                release.notified().await;
                cache.update(&serial, value).await;
            }),
        );
        running.await.unwrap();
        finished
    }

    #[tokio::test]
    async fn report_forwarder_retries_initial_subscribe_failure() {
        let transport = FakeMqttTransport::with_subscribe_failures(1);
        let (sender, _receiver) = mpsc::channel(1);
        let task = tokio::spawn(crate::machine::runtime::forward_print_reports_with_retry(
            test_config(),
            transport.clone(),
            runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
            Duration::from_secs(1),
            sender,
            Duration::from_millis(1),
            crate::machine::DeviceFeatureCache::default(),
        ));

        tokio::time::timeout(Duration::from_secs(1), async {
            while transport.subscribe_attempts().await < 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        task.abort();
    }

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
                device_features: None,
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
                device_features: None,
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
                device_features: None,
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
                device_features: None,
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

        fn new_with_feature(fun: &'static str) -> Self {
            Self {
                state: std::sync::Arc::new(PausedMqttTransportState {
                    blocked: Notify::new(),
                    release: Notify::new(),
                    reports: Mutex::new(vec![
                        get_version_report("X1 Carbon"),
                        runtime_feature_report("READY", fun),
                    ]),
                    pause_first_report: true,
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
