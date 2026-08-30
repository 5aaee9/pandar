use super::*;

#[test]
fn interpretation_accepts_bambu_hms_shape_without_dropping_progress() {
    let report = serde_json::json!({
        "print": {
            "gcode_state": "RUNNING",
            "mc_percent": 37,
            "mc_remaining_time": 52,
            "hms": [{
                "attr": 0x07FF0200,
                "code": 0x8011
            }]
        }
    });

    let progress = print_report_from_json(&endpoint(), &report);

    assert_eq!(progress.gcode_state.as_deref(), Some("RUNNING"));
    assert_eq!(progress.percent, Some(37));
    assert_eq!(progress.remaining_time_minutes, Some(52));
    assert_eq!(
        progress.hms,
        Some(vec![MachineHmsItem {
            attr: 0x07FF0200,
            code: 0x8011,
        }])
    );
    assert_eq!(progress.diagnostics.len(), 1);
    assert_eq!(progress.diagnostics[0].kind, "hms");
    assert_eq!(progress.diagnostics[0].code.as_deref(), Some("8011"));
    assert_eq!(progress.diagnostics[0].message, "");
    assert_eq!(
        serde_json::to_value(&progress.diagnostics[0].payload).unwrap(),
        serde_json::json!({
            "attr": 0x07FF0200,
            "code": 0x8011
        })
    );
}

#[test]
fn print_report_empty_hms_preserves_snapshot_presence_in_event() {
    let report = serde_json::json!({
        "print": {
            "hms": []
        }
    });
    let config = AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
    };

    let progress = print_report_from_json(&endpoint(), &report);

    assert_eq!(progress.hms, Some(Vec::new()));
    let event = print_job_report_event(&config, progress);
    let Some(agent_event::Event::PrintJobReport(report)) = event.event else {
        panic!("expected print job report event");
    };
    assert!(report.has_hms);
    assert!(report.hms.is_empty());
}

#[test]
fn print_report_malformed_hms_does_not_clear_snapshot_or_drop_progress() {
    let report = serde_json::json!({
        "print": {
            "gcode_state": "RUNNING",
            "hms": [{
                "attr": 0x07FF0200
            }]
        }
    });

    let progress = print_report_from_json(&endpoint(), &report);

    assert_eq!(progress.gcode_state.as_deref(), Some("RUNNING"));
    assert_eq!(progress.hms, None);
    assert!(progress.diagnostics.is_empty());
}

#[test]
fn print_report_legacy_hms_diagnostic_does_not_clear_snapshot() {
    let report = serde_json::json!({
        "print": {
            "hms": [{
                "code": "0300_0A00_0001_0002",
                "message": "fan speed is low"
            }]
        }
    });

    let progress = print_report_from_json(&endpoint(), &report);

    assert_eq!(progress.hms, None);
    assert_eq!(progress.diagnostics.len(), 1);
    assert_eq!(
        progress.diagnostics[0].code.as_deref(),
        Some("0300_0A00_0001_0002")
    );
    assert_eq!(progress.diagnostics[0].message, "fan speed is low");
}
