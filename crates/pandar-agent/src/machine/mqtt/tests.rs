use std::time::Duration;

use serde_json::json;

use super::*;
use crate::machine::BambuPrinterEndpoint;

fn endpoint() -> BambuPrinterEndpoint {
    BambuPrinterEndpoint {
        host: "192.0.2.10".to_string(),
        serial: "01S00EXAMPLE".to_string(),
        access_code: "12345678".to_string(),
        model: Some("A1 Mini".to_string()),
        name: Some("garage-a1".to_string()),
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
fn lan_tls_uses_rustls_certificate_policy_for_printer_certificates() {
    assert!(matches!(
        bambu_lan_tls_config(),
        TlsConfiguration::Rustls(_)
    ));
}

#[test]
fn pushall_payload_matches_reference() {
    assert_eq!(
        BambuMqttCommand::RequestPushAll.payload(),
        json!({"pushing": {"command": "pushall"}})
    );
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
fn report_maps_to_snapshot_with_config_identity() {
    let report = json!({"print": {"gcode_state": "RUNNING"}});

    assert_eq!(
        snapshot_from_report(&endpoint(), &report),
        MachineSnapshot {
            serial: "01S00EXAMPLE".to_string(),
            name: "garage-a1".to_string(),
            model: Some("A1 Mini".to_string()),
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
    let transport = FakeMqttTransport::with_reports([json!({
        "print": {"gcode_state": "RUNNING"}
    })]);

    let snapshot = refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(
        snapshot,
        MachineSnapshot {
            serial: "01S00EXAMPLE".to_string(),
            name: "garage-a1".to_string(),
            model: Some("A1 Mini".to_string()),
            state: "RUNNING".to_string(),
        }
    );
    assert_eq!(
        transport.subscriptions().await,
        ["device/01S00EXAMPLE/report".to_string()]
    );
    assert_eq!(
        transport.published_commands().await,
        [PublishedMqttCommand {
            topic: "device/01S00EXAMPLE/request".to_string(),
            payload: json!({"pushing": {"command": "pushall"}}),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn refresh_timeout_error_includes_serial_context() {
    let transport = FakeMqttTransport::with_timeout();

    let err = refresh_printer(&transport, &endpoint(), Duration::from_millis(1))
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("refresh printer 01S00EXAMPLE"));
    assert!(format!("{err:#}").contains("timed out waiting for MQTT report"));
}
