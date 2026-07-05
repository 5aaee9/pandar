use serde_json::json;

use super::*;
use crate::machine::MachineSnapshot;

#[test]
fn report_maps_to_snapshot_uses_configured_model() {
    let report = json!({"print": {"gcode_state": "RUNNING"}});

    assert_eq!(
        snapshot_from_report(&endpoint(), &report),
        MachineSnapshot {
            serial: "01S00EXAMPLE".to_string(),
            host: Some("192.0.2.10".to_string()),
            access_code: Some("12345678".to_string()),
            name: "garage-a1".to_string(),
            model: Some("A1 Mini".to_string()),
            state: "RUNNING".to_string(),
            nozzle_temperatures: Vec::new(),
            active_nozzle: None,
            bed_temperature_celsius: None,
            bed_target_temperature_celsius: None,
            chamber_temperature_celsius: None,
            chamber_light_on: None,
        }
    );
}

#[test]
fn report_maps_chamber_light_state_to_snapshot() {
    let report = json!({
        "print": {
            "lights_report": [{"node": "chamber_light", "mode": "on"}]
        }
    });

    assert_eq!(
        snapshot_from_report(&endpoint(), &report).chamber_light_on,
        Some(true)
    );
}

#[test]
fn report_maps_to_snapshot_without_configured_model() {
    let mut endpoint = endpoint();
    endpoint.model = None;

    assert_eq!(
        snapshot_from_report(&endpoint, &json!({"print": {"gcode_state": "RUNNING"}})).model,
        None,
    );
}

#[test]
fn report_maps_temperatures_to_snapshot() {
    let report = json!({
        "print": {
            "gcode_state": "RUNNING",
            "nozzle_temper": 41,
            "nozzle_target_temper": 220,
            "nozzle_temper2": 42,
            "nozzle_target_temper2": 230,
            "bed_temper": 60,
            "bed_target_temper": 65,
            "chamber_temper": 32
        }
    });

    let snapshot = snapshot_from_report(&endpoint(), &report);

    assert_eq!(snapshot.nozzle_temperatures.len(), 2);
    assert_eq!(snapshot.nozzle_temperatures[0].label.as_deref(), Some("L"));
    assert_eq!(
        snapshot.nozzle_temperatures[0].current_celsius.as_deref(),
        Some("41")
    );
    assert_eq!(
        snapshot.nozzle_temperatures[0].target_celsius.as_deref(),
        Some("220")
    );
    assert_eq!(snapshot.nozzle_temperatures[1].label.as_deref(), Some("R"));
    assert_eq!(snapshot.bed_temperature_celsius.as_deref(), Some("60"));
    assert_eq!(
        snapshot.bed_target_temperature_celsius.as_deref(),
        Some("65")
    );
    assert_eq!(snapshot.chamber_temperature_celsius.as_deref(), Some("32"));
}

#[test]
fn report_maps_bambu_studio_v2_temperatures_to_snapshot() {
    let report = json!({
        "print": {
            "gcode_state": "RUNNING",
            "device": {
                "bed_temp": (65 << 16) | 60,
                "ctc": {
                    "state": 1,
                    "info": {
                        "temp": (45 << 16) | 32
                    }
                },
                "extruder": {
                    "state": 0x0012,
                    "info": [
                        {"id": 0, "info": 8, "temp": (220 << 16) | 27},
                        {"id": 1, "info": 8, "temp": (215 << 16) | 22}
                    ]
                }
            }
        }
    });

    let snapshot = snapshot_from_report(&endpoint(), &report);

    assert_eq!(snapshot.nozzle_temperatures.len(), 2);
    assert_eq!(snapshot.nozzle_temperatures[0].label.as_deref(), Some("L"));
    assert_eq!(
        snapshot.nozzle_temperatures[0].current_celsius.as_deref(),
        Some("22")
    );
    assert_eq!(
        snapshot.nozzle_temperatures[0].target_celsius.as_deref(),
        Some("215")
    );
    assert_eq!(snapshot.nozzle_temperatures[1].label.as_deref(), Some("R"));
    assert_eq!(
        snapshot.nozzle_temperatures[1].current_celsius.as_deref(),
        Some("27")
    );
    assert_eq!(snapshot.active_nozzle.as_deref(), Some("L"));
    assert_eq!(snapshot.bed_temperature_celsius.as_deref(), Some("60"));
    assert_eq!(
        snapshot.bed_target_temperature_celsius.as_deref(),
        Some("65")
    );
    assert_eq!(snapshot.chamber_temperature_celsius.as_deref(), Some("32"));
}

#[test]
fn report_maps_bambu_studio_v2_active_right_nozzle_to_snapshot() {
    let report = json!({
        "print": {
            "device": {
                "extruder": {
                    "state": 0x0002,
                    "info": [
                        {"id": 0, "temp": 27},
                        {"id": 1, "temp": 22}
                    ]
                }
            }
        }
    });

    let snapshot = snapshot_from_report(&endpoint(), &report);

    assert_eq!(snapshot.active_nozzle.as_deref(), Some("R"));
}

#[test]
fn report_ignores_bambu_studio_v2_target_nozzle_for_active_snapshot() {
    let report = json!({
        "print": {
            "device": {
                "extruder": {
                    "state": 0x0102,
                    "info": [
                        {"id": 0, "temp": 27},
                        {"id": 1, "temp": 22}
                    ]
                }
            }
        }
    });

    let snapshot = snapshot_from_report(&endpoint(), &report);

    assert_eq!(snapshot.active_nozzle.as_deref(), Some("R"));
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
