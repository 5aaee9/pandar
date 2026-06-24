use std::time::Duration;

use serde_json::json;

use super::*;
use crate::machine::{
    file_transfer::{
        FakeMachineFileTransfer, FileTransferRequest, TransferProtectionMode::ProtectedData,
    },
    mqtt::FakeMqttTransport,
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

fn get_version_report(model: &str) -> serde_json::Value {
    json!({
        "info": {
            "command": "get_version",
            "module": [{"name": "ota", "product_name": model}]
        }
    })
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

    let snapshots = gateway.refresh_printers().await.unwrap();

    assert_eq!(
        snapshots,
        vec![
            MachineSnapshot {
                serial: "SERIAL1".to_string(),
                name: "printer-SERIAL1".to_string(),
                model: Some("P2S".to_string()),
                state: "READY".to_string(),
            },
            MachineSnapshot {
                serial: "SERIAL2".to_string(),
                name: "printer-SERIAL2".to_string(),
                model: Some("X1 Carbon".to_string()),
                state: "IDLE".to_string(),
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
    assert_eq!(
        first.published_commands().await,
        [
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: json!({"info": {"command": "get_version", "sequence_id": "90002"}}),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: json!({"pushing": {"command": "pushall"}}),
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
        vec![(ProtectedData, FileTransferRequest::upload("plate.3mf", 3))]
    );
    assert_eq!(
        mqtt.published_commands().await,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({
                "print": {
                    "command": "project_file",
                    "sequence_id": "20000",
                    "param": "Metadata/plate_1.gcode",
                    "url": "ftp://plate.3mf",
                    "file": "plate.3mf",
                    "task_id": "job-1",
                    "subtask_id": "artifact-1",
                    "use_ams": true,
                    "flow_cali": false,
                    "timelapse": true
                }
            }),
            qos: BAMBU_MQTT_QOS,
        }]
    );
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
    }
}
