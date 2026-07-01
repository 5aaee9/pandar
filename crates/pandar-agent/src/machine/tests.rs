use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use tokio::sync::{Mutex, Notify, mpsc};

use super::*;
use crate::AgentConfig;
use crate::machine::{
    file_transfer::{
        FakeMachineFileTransfer, FileTransferRequest, TransferProtectionMode::ProtectedData,
    },
    mqtt::{BambuMqttTransport, FakeMqttTransport, PublishedMqttCommand},
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
    json!({
        "info": {
            "command": "get_version",
            "module": [{"name": "ota", "product_name": model}]
        }
    })
}

fn runtime_reports(model: &str, state: &str) -> [serde_json::Value; 2] {
    [
        get_version_report(model),
        json!({"print": {"state": state}}),
    ]
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

    assert_eq!(
        mqtt.published_commands().await,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {"command": "pause", "sequence_id": "0"}}),
            qos: BAMBU_MQTT_QOS,
        }]
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

    assert_eq!(
        mqtt.published_commands().await,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {"command": "print_speed", "param": "4", "sequence_id": "0"}}),
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

    assert_eq!(
        mqtt.published_commands().await,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {"command": "gcode_line", "param": "G28", "sequence_id": "90001"}}),
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

    assert_eq!(
        mqtt.published_commands().await,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {"command": "gcode_line", "param": "G91\nG0 X10 Z-0.5 F3000\nG90", "sequence_id": "90001"}}),
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
            },
        )
        .await
        .unwrap();

    assert_eq!(
        mqtt.published_commands().await,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: json!({"print": {"command": "gcode_line", "param": "M109 S215", "sequence_id": "90001"}}),
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
            gateway.refresh_printers().await.unwrap(),
            vec![MachineSnapshot {
                serial: "SERIAL1".to_string(),
                name: "office".to_string(),
                model: Some("X1 Carbon".to_string()),
                state: "IDLE".to_string(),
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
            gateway.refresh_printers().await.unwrap(),
            vec![MachineSnapshot {
                serial: "SERIAL1".to_string(),
                name: "new office".to_string(),
                model: Some("P2S".to_string()),
                state: "PAUSED".to_string(),
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
            gateway.refresh_printers().await.unwrap(),
            vec![MachineSnapshot {
                serial: "SERIAL1".to_string(),
                name: "old office".to_string(),
                model: Some("X1 Carbon".to_string()),
                state: "IDLE".to_string(),
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
            gateway.refresh_printers().await.unwrap(),
            vec![MachineSnapshot {
                serial: "SERIAL1".to_string(),
                name: "old office".to_string(),
                model: Some("X1 Carbon".to_string()),
                state: "IDLE".to_string(),
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
                        json!({"print": {"state": "READY"}}),
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
                        json!({"print": {"state": state}}),
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
