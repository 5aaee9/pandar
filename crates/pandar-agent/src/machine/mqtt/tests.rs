use std::{
    io::{self, Write},
    sync::{Arc, Mutex as StdMutex},
    time::Duration,
};

use serde_json::json;
use tokio::sync::mpsc;

use super::*;
use crate::machine::BambuPrinterEndpoint;
use crate::{
    AgentConfig,
    protocol::agent::v1::{PrintJobReport, agent_event},
};

fn endpoint() -> BambuPrinterEndpoint {
    BambuPrinterEndpoint {
        host: "192.0.2.10".to_string(),
        serial: "01S00EXAMPLE".to_string(),
        access_code: "12345678".to_string(),
        model: Some("A1 Mini".to_string()),
        name: Some("garage-a1".to_string()),
    }
}

fn get_version_report(model: &str) -> serde_json::Value {
    json!({
        "info": {
            "command": "get_version",
            "module": [
                {"name": "wifi", "product_name": "ignored"},
                {"name": "ota", "sw_ver": "01.08.01.00", "product_name": model, "sn": "01S00EXAMPLE"}
            ]
        }
    })
}

fn request_command(payload: serde_json::Value) -> PublishedMqttCommand {
    PublishedMqttCommand {
        topic: "device/01S00EXAMPLE/request".to_string(),
        payload,
        qos: BAMBU_MQTT_QOS,
    }
}

#[test]
fn topics_match_bambu_reference_shape() {
    let topics = BambuMqttTopics::for_serial("01S00EXAMPLE");

    assert_eq!(topics.report, "device/01S00EXAMPLE/report");
    assert_eq!(topics.request, "device/01S00EXAMPLE/request");
}

#[test]
fn constants_match_bambu_defaults() {
    assert_eq!(BAMBU_MQTT_PORT, 8883);
    assert_eq!(BAMBU_MQTT_USERNAME, "bblp");
    assert_eq!(BAMBU_MQTT_QOS, 1);
}

#[test]
fn lan_mqtt_accepts_full_pushall_reports() {
    let options = bambu_lan_mqtt_options(&endpoint(), None);

    assert!(options.max_packet_size() >= 256 * 1024);
}

#[test]
fn mqtt_report_error_log_preserves_error_chain() {
    let logs = CapturedLogs::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.clone())
        .with_ansi(false)
        .without_time()
        .finish();
    let err = anyhow!("payload size limit exceeded: 262600")
        .context("MQTT serialization/deserialization error")
        .context("poll rumqttc event loop");

    tracing::subscriber::with_default(subscriber, || warn_mqtt_report_receive_failed(&err));

    let captured = logs.contents();
    assert!(captured.contains("MQTT report receive failed"));
    assert!(captured.contains("payload size limit exceeded: 262600"));
    assert!(captured.contains("poll rumqttc event loop"));
}

#[test]
fn lan_tls_uses_rustls_certificate_policy_for_printer_certificates() {
    assert!(matches!(
        bambu_lan_tls_config(),
        TlsConfiguration::Rustls(_)
    ));
}

#[test]
fn ftps_lan_tls_default_profile_config_constructs() {
    let config = crate::machine::ftps::bambu_lan_ftps_tls_config_for_default_profile();

    assert!(config.alpn_protocols.is_empty());
}

#[test]
fn pushall_payload_matches_reference() {
    assert_eq!(
        BambuMqttCommand::RequestPushAll.payload(),
        json!({"pushing": {"command": "pushall"}})
    );
}

#[test]
fn get_version_payload_matches_reference() {
    assert_eq!(
        BambuMqttCommand::GetVersion.payload(),
        json!({"info": {"command": "get_version", "sequence_id": "90002"}})
    );
}

#[test]
fn get_version_report_extracts_trimmed_ota_model() {
    assert_eq!(
        model_from_get_version_report(&get_version_report(" P2S ")).unwrap(),
        "P2S"
    );
}

#[test]
fn get_version_report_rejects_missing_model() {
    let report = json!({
        "info": {
            "command": "get_version",
            "module": [{"name": "ota", "product_name": "   "}]
        }
    });

    assert!(model_from_get_version_report(&report).is_err());
}

#[test]
fn basic_print_control_payloads_match_reference() {
    assert_eq!(
        BambuMqttCommand::PausePrint.payload(),
        json!({"print": {"command": "pause", "sequence_id": "0"}})
    );
    assert_eq!(
        BambuMqttCommand::ResumePrint.payload(),
        json!({"print": {"command": "resume", "sequence_id": "0"}})
    );
    assert_eq!(
        BambuMqttCommand::StopPrint.payload(),
        json!({"print": {"command": "stop", "sequence_id": "0"}})
    );
}

#[test]
fn print_speed_is_limited_to_reference_modes() {
    assert_eq!(
        BambuMqttCommand::SetPrintSpeed(PrintSpeed::new(4).unwrap()).payload(),
        json!({"print": {"command": "print_speed", "param": "4", "sequence_id": "0"}})
    );
    assert!(PrintSpeed::new(0).is_err());
    assert!(PrintSpeed::new(5).is_err());
}

#[test]
fn gcode_line_payload_preserves_single_home_line() {
    assert_eq!(
        BambuMqttCommand::GcodeLine(GcodeLineCommand {
            lines: vec!["G28".to_string()],
        })
        .payload(),
        json!({"print": {"command": "gcode_line", "param": "G28", "sequence_id": "90001"}})
    );
}

#[test]
fn gcode_line_payload_joins_relative_move_lines() {
    assert_eq!(
        BambuMqttCommand::GcodeLine(GcodeLineCommand {
            lines: vec![
                "G91".to_string(),
                "G0 X10 Z-0.5 F3000".to_string(),
                "G90".to_string(),
            ],
        })
        .payload(),
        json!({"print": {"command": "gcode_line", "param": "G91\nG0 X10 Z-0.5 F3000\nG90", "sequence_id": "90001"}})
    );
}

#[test]
fn gcode_line_payload_preserves_hotend_temperature_line() {
    assert_eq!(
        BambuMqttCommand::GcodeLine(GcodeLineCommand {
            lines: vec!["M104 S200".to_string()],
        })
        .payload(),
        json!({"print": {"command": "gcode_line", "param": "M104 S200", "sequence_id": "90001"}})
    );
}

#[test]
fn raw_json_payload_is_preserved() {
    let payload = json!({"print": {"command": "custom", "sequence_id": "9"}});
    assert_eq!(
        BambuMqttCommand::RawJson(payload.clone()).payload(),
        payload
    );
}

#[test]
fn project_file_payload_reserves_dispatch_identity_and_flags() {
    let payload = BambuMqttCommand::ProjectFile(ProjectFileCommand {
        filename: "job.3mf".to_string(),
        plate_id: 2,
        task_id: "task-1".to_string(),
        subtask_id: "subtask-1".to_string(),
        use_ams: true,
        flow_cali: true,
        timelapse: false,
        ams_mapping_json: None,
        ams_mapping2_json: None,
    })
    .payload();

    assert_eq!(
        payload,
        json!({
            "print": {
                "command": "project_file",
                "sequence_id": "20000",
                "param": "Metadata/plate_2.gcode",
                "url": "ftp://job.3mf",
                "file": "job.3mf",
                "task_id": "task-1",
                "subtask_id": "subtask-1",
                "use_ams": true,
                "flow_cali": true,
                "timelapse": false
            }
        })
    );
}

#[test]
fn project_file_payload_omits_mapping_keys_when_no_mapping_supplied() {
    let payload = BambuMqttCommand::ProjectFile(ProjectFileCommand {
        filename: "job.3mf".to_string(),
        plate_id: 2,
        task_id: "task-1".to_string(),
        subtask_id: "subtask-1".to_string(),
        use_ams: false,
        flow_cali: false,
        timelapse: false,
        ams_mapping_json: None,
        ams_mapping2_json: None,
    })
    .payload();

    assert!(payload["print"].get("ams_mapping").is_none());
    assert!(payload["print"].get("ams_mapping_2").is_none());
    assert_eq!(payload["print"]["use_ams"], false);
}

#[test]
fn project_file_payload_includes_ams_mapping_only_when_supplied() {
    let payload = BambuMqttCommand::ProjectFile(ProjectFileCommand {
        filename: "job.3mf".to_string(),
        plate_id: 2,
        task_id: "task-1".to_string(),
        subtask_id: "subtask-1".to_string(),
        use_ams: true,
        flow_cali: false,
        timelapse: false,
        ams_mapping_json: Some("[0,-1,4]".to_string()),
        ams_mapping2_json: None,
    })
    .payload();

    assert_eq!(payload["print"]["ams_mapping"], json!([0, -1, 4]));
    assert!(payload["print"].get("ams_mapping_2").is_none());
    assert_eq!(payload["print"]["use_ams"], true);
}

#[test]
fn project_file_payload_includes_ams_mapping2_only_when_supplied() {
    let payload = BambuMqttCommand::ProjectFile(ProjectFileCommand {
        filename: "job.3mf".to_string(),
        plate_id: 2,
        task_id: "task-1".to_string(),
        subtask_id: "subtask-1".to_string(),
        use_ams: true,
        flow_cali: false,
        timelapse: false,
        ams_mapping_json: None,
        ams_mapping2_json: Some(r#"[{"ams_id":255,"slot_id":0}]"#.to_string()),
    })
    .payload();

    assert!(payload["print"].get("ams_mapping").is_none());
    assert_eq!(
        payload["print"]["ams_mapping_2"],
        json!([{"ams_id": 255, "slot_id": 0}])
    );
}

#[test]
fn project_file_payload_includes_both_mapping_keys_when_supplied() {
    let payload = BambuMqttCommand::ProjectFile(ProjectFileCommand {
        filename: "job.3mf".to_string(),
        plate_id: 2,
        task_id: "task-1".to_string(),
        subtask_id: "subtask-1".to_string(),
        use_ams: true,
        flow_cali: false,
        timelapse: false,
        ams_mapping_json: Some("[0,1]".to_string()),
        ams_mapping2_json: Some(r#"[{"ams_id":0,"slot_id":1}]"#.to_string()),
    })
    .payload();

    assert_eq!(payload["print"]["ams_mapping"], json!([0, 1]));
    assert_eq!(
        payload["print"]["ams_mapping_2"],
        json!([{"ams_id": 0, "slot_id": 1}])
    );
}

#[test]
fn project_file_payload_rewrites_flat_external_mapping_values() {
    let payload = BambuMqttCommand::ProjectFile(ProjectFileCommand {
        filename: "job.3mf".to_string(),
        plate_id: 2,
        task_id: "task-1".to_string(),
        subtask_id: "subtask-1".to_string(),
        use_ams: true,
        flow_cali: false,
        timelapse: false,
        ams_mapping_json: Some("[254,255,15]".to_string()),
        ams_mapping2_json: None,
    })
    .payload();

    assert_eq!(payload["print"]["ams_mapping"], json!([-1, -1, 15]));
}

#[test]
fn report_maps_to_snapshot_without_configured_model() {
    let report = json!({"print": {"gcode_state": "RUNNING"}});

    assert_eq!(
        snapshot_from_report(&endpoint(), &report),
        MachineSnapshot {
            serial: "01S00EXAMPLE".to_string(),
            name: "garage-a1".to_string(),
            model: None,
            state: "RUNNING".to_string(),
        }
    );
}

#[test]
fn report_state_falls_back_to_print_state() {
    let report = json!({"print": {"state": "READY"}});

    assert_eq!(snapshot_from_report(&endpoint(), &report).state, "READY");
}

#[test]
fn report_state_falls_back_to_root_state() {
    let report = json!({"state": "IDLE"});

    assert_eq!(snapshot_from_report(&endpoint(), &report).state, "IDLE");
}

#[test]
fn report_state_skips_non_string_candidates() {
    let report = json!({"print": {"gcode_state": 123, "state": "READY"}});

    assert_eq!(snapshot_from_report(&endpoint(), &report).state, "READY");
}

#[test]
fn report_state_defaults_to_unknown() {
    let report = json!({"print": {"gcode_state": 123}});

    assert_eq!(snapshot_from_report(&endpoint(), &report).state, "unknown");
}

#[test]
fn report_name_defaults_to_serial() {
    let mut endpoint = endpoint();
    endpoint.name = None;

    assert_eq!(
        snapshot_from_report(&endpoint, &json!({})).name,
        "01S00EXAMPLE"
    );
}

#[tokio::test]
async fn refresh_subscribes_publishes_and_maps_report() {
    let mut endpoint = endpoint();
    endpoint.model = Some("Configured Model".to_string());
    let transport = FakeMqttTransport::with_reports([
        get_version_report("P2S"),
        json!({"print": {"gcode_state": "RUNNING"}}),
    ]);

    let snapshot = refresh_printer(&transport, &endpoint, Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(
        snapshot,
        MachineSnapshot {
            serial: "01S00EXAMPLE".to_string(),
            name: "garage-a1".to_string(),
            model: Some("P2S".to_string()),
            state: "RUNNING".to_string(),
        }
    );
    assert_eq!(
        transport.subscriptions().await,
        ["device/01S00EXAMPLE/report".to_string()]
    );
    assert_eq!(
        transport.published_commands().await,
        [
            request_command(json!({"info": {"command": "get_version", "sequence_id": "90002"}})),
            request_command(json!({"pushing": {"command": "pushall"}})),
        ]
    );
}

#[tokio::test]
async fn refresh_ignores_unrelated_reports_before_get_version() {
    let transport = FakeMqttTransport::with_reports([
        json!({"print": {"gcode_state": "STALE"}}),
        json!({"info": {"command": "other"}}),
        get_version_report("X1 Carbon"),
        json!({"print": {"state": "READY"}}),
    ]);

    let snapshot = refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(snapshot.model.as_deref(), Some("X1 Carbon"));
    assert_eq!(snapshot.state, "READY");
    assert_eq!(
        transport.published_commands().await,
        [
            request_command(json!({"info": {"command": "get_version", "sequence_id": "90002"}})),
            request_command(json!({"pushing": {"command": "pushall"}})),
        ]
    );
}

#[tokio::test]
async fn refresh_timeout_error_includes_serial_context() {
    let transport = FakeMqttTransport::with_timeout();

    let err = refresh_printer(&transport, &endpoint(), Duration::from_millis(1))
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("refresh printer 01S00EXAMPLE"));
    assert!(format!("{err:#}").contains("wait for MQTT get_version report"));
    assert!(transport.published_commands().await.len() == 1);
}

#[tokio::test]
async fn refresh_fails_total_get_version_deadline_when_unrelated_reports_continue() {
    let transport = FakeMqttTransport::with_infinite_unrelated_reports();

    let err = refresh_printer(&transport, &endpoint(), Duration::from_millis(10))
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("timed out waiting for MQTT get_version report"));
    assert_eq!(
        transport.published_commands().await,
        [request_command(
            json!({"info": {"command": "get_version", "sequence_id": "90002"}})
        )]
    );
}

#[tokio::test]
async fn refresh_missing_model_fails_before_pushall() {
    let transport = FakeMqttTransport::with_reports([json!({
        "info": {
            "command": "get_version",
            "module": [{"name": "ota", "product_name": "   "}]
        }
    })]);

    let err = refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("missing ota product_name"));
    assert_eq!(
        transport.published_commands().await,
        [request_command(
            json!({"info": {"command": "get_version", "sequence_id": "90002"}})
        )]
    );
}

#[tokio::test]
async fn refresh_get_version_publish_failure_fails_before_pushall() {
    let transport = FakeMqttTransport::with_publish_failure(BambuMqttCommand::GetVersion.payload());

    let err = refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("publish get_version"));
    assert!(format!("{err:#}").contains("fake publish failure"));
    assert!(transport.published_commands().await.is_empty());
}

#[tokio::test]
async fn refresh_discovery_failure_log_includes_serial_and_error_chain() {
    let logs = CapturedLogs::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.clone())
        .with_ansi(false)
        .without_time()
        .finish();
    let transport = FakeMqttTransport::with_publish_failure(BambuMqttCommand::GetVersion.payload());

    let _guard = tracing::subscriber::set_default(subscriber);
    refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
        .await
        .unwrap_err();

    let captured = logs.contents();
    assert!(captured.contains("printer model discovery failed"));
    assert!(captured.contains("01S00EXAMPLE"));
    assert!(captured.contains("publish get_version to request topic"));
    assert!(captured.contains("fake publish failure"));
}

#[test]
fn print_report_from_report_extracts_progress_and_diagnostics() {
    let report = json!({
        "print": {
            "task_id": "job-123",
            "subtask_id": "artifact-456",
            "gcode_state": "RUNNING",
            "mc_percent": "42",
            "mc_remaining_time": 87,
            "layer_num": "12",
            "total_layer_num": 120,
            "gcode_file": "plate_1.gcode",
            "subtask_name": "drawer-organizer",
            "print_error": "nozzle temperature error",
            "hms": [
                {"code": "0300_0A00_0001_0002", "message": "fan speed is low"}
            ]
        }
    });

    let progress = print_report_from_report(&endpoint(), &report);

    assert_eq!(progress.serial, "01S00EXAMPLE");
    assert_eq!(progress.job_id.as_deref(), Some("job-123"));
    assert_eq!(progress.artifact_id.as_deref(), Some("artifact-456"));
    assert_eq!(progress.subtask_id.as_deref(), Some("artifact-456"));
    assert_eq!(progress.gcode_state.as_deref(), Some("RUNNING"));
    assert_eq!(progress.percent, Some(42));
    assert_eq!(progress.remaining_time_minutes, Some(87));
    assert_eq!(progress.current_layer, Some(12));
    assert_eq!(progress.total_layers, Some(120));
    assert_eq!(progress.gcode_file.as_deref(), Some("plate_1.gcode"));
    assert_eq!(progress.subtask_name.as_deref(), Some("drawer-organizer"));
    assert_eq!(progress.diagnostics.len(), 2);
    assert_eq!(progress.diagnostics[0].kind, "print_error");
    assert_eq!(progress.diagnostics[0].severity, "error");
    assert_eq!(progress.diagnostics[0].message, "nozzle temperature error");
    assert_eq!(progress.diagnostics[1].kind, "hms");
    assert_eq!(progress.diagnostics[1].severity, "warning");
    assert_eq!(
        progress.diagnostics[1].code.as_deref(),
        Some("0300_0A00_0001_0002")
    );
    assert_eq!(progress.diagnostics[1].message, "fan speed is low");
    assert!(!progress.observed_at.is_empty());
}

#[test]
fn print_report_from_report_drops_out_of_range_numeric_values() {
    let report = json!({
        "print": {
            "mc_percent": "101",
            "mc_remaining_time": 4321,
            "layer_num": "100001",
            "total_layer_num": -1
        }
    });

    let progress = print_report_from_report(&endpoint(), &report);

    assert_eq!(progress.percent, None);
    assert_eq!(progress.remaining_time_minutes, None);
    assert_eq!(progress.current_layer, None);
    assert_eq!(progress.total_layers, None);
}

#[test]
fn print_job_report_event_sets_numeric_presence_booleans() {
    let config = AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: ".".into(),
    };
    let progress = PrintReportProgress {
        serial: "01S00EXAMPLE".to_owned(),
        job_id: Some("job-123".to_owned()),
        artifact_id: None,
        subtask_id: None,
        gcode_state: Some("RUNNING".to_owned()),
        percent: Some(0),
        remaining_time_minutes: None,
        current_layer: Some(7),
        total_layers: None,
        gcode_file: None,
        subtask_name: None,
        diagnostics: Vec::new(),
        observed_at: "2026-06-22T00:00:00Z".to_owned(),
        printer_materials_json: String::new(),
    };

    let event = print_job_report_event(&config, progress);

    assert_eq!(event.agent_id, "agent-id");
    assert_eq!(event.tenant_id, "tenant-id");
    let Some(agent_event::Event::PrintJobReport(PrintJobReport {
        percent,
        has_percent,
        remaining_time_minutes,
        has_remaining_time_minutes,
        current_layer,
        has_current_layer,
        total_layers,
        has_total_layers,
        printer_materials_json,
        ..
    })) = event.event
    else {
        panic!("expected print job report event");
    };
    assert_eq!(percent, 0);
    assert!(has_percent);
    assert_eq!(remaining_time_minutes, 0);
    assert!(!has_remaining_time_minutes);
    assert_eq!(current_layer, 7);
    assert!(has_current_layer);
    assert_eq!(total_layers, 0);
    assert!(!has_total_layers);
    assert!(printer_materials_json.is_empty());
}

#[test]
fn print_report_from_report_populates_printer_materials_json() {
    let report = json!({
        "print": {
            "ams": {
                "tray_now": 254,
                "vt_tray": {"tray_info_idx": "GFL05", "tray_color": "#abcdef"}
            }
        }
    });

    let progress = print_report_from_report(&endpoint(), &report);
    let materials: serde_json::Value =
        serde_json::from_str(&progress.printer_materials_json).unwrap();

    assert_eq!(materials["external_spools"][0]["external_id"], "254");
    assert_eq!(materials["external_spools"][0]["filament_id"], "GFL05");
    assert_eq!(materials["external_spools"][0]["color"], "ABCDEF");
    assert_eq!(materials["active_tray"]["kind"], "external");
}

#[tokio::test]
async fn forward_print_reports_uses_transport_without_live_socket() {
    let transport = FakeMqttTransport::with_reports([json!({
        "print": {
            "task_id": "job-123",
            "subtask_id": "artifact-456",
            "gcode_state": "RUNNING",
            "mc_percent": 55
        }
    })]);
    let (sender, mut receiver) = mpsc::channel(4);
    let config = AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: ".".into(),
    };
    let endpoint = endpoint();
    let forwarder = tokio::spawn({
        let config = config.clone();
        let transport = transport.clone();
        let endpoint = endpoint.clone();
        async move {
            forward_print_reports(
                &config,
                &transport,
                &endpoint,
                Duration::from_millis(1),
                &sender,
            )
            .await
        }
    });

    let event = receiver.recv().await.unwrap();
    drop(receiver);
    forwarder.await.unwrap().unwrap();

    let Some(agent_event::Event::PrintJobReport(report)) = event.event else {
        panic!("expected print job report event");
    };
    assert_eq!(report.serial, "01S00EXAMPLE");
    assert_eq!(report.job_id, "job-123");
    assert_eq!(report.artifact_id, "artifact-456");
    assert_eq!(report.subtask_id, "artifact-456");
    assert_eq!(report.percent, 55);
    assert!(report.has_percent);
    assert!(report.printer_materials_json.is_empty());
    assert_eq!(
        transport.subscriptions().await,
        ["device/01S00EXAMPLE/report".to_string()]
    );
}

#[derive(Clone, Default)]
struct CapturedLogs {
    buffer: Arc<StdMutex<Vec<u8>>>,
}

impl CapturedLogs {
    fn contents(&self) -> String {
        String::from_utf8(self.buffer.lock().unwrap().clone()).unwrap()
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedLogs {
    type Writer = CapturedLogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        CapturedLogWriter {
            buffer: Arc::clone(&self.buffer),
        }
    }
}

struct CapturedLogWriter {
    buffer: Arc<StdMutex<Vec<u8>>>,
}

impl Write for CapturedLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
