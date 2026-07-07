use super::*;
use crate::machine::MachineSnapshot;

mod fixtures;

use fixtures::*;

#[test]
fn report_maps_to_snapshot_uses_configured_model() {
    let report = report_with_print(SnapshotPrintFixture {
        gcode_state: Some(ScalarFixture::Text("RUNNING")),
        ..Default::default()
    });

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
    let report = report_with_print(SnapshotPrintFixture {
        lights_report: vec![LightReportFixture {
            node: "chamber_light",
            mode: "on",
        }],
        ..Default::default()
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
        snapshot_from_report(
            &endpoint,
            &report_with_print(SnapshotPrintFixture {
                gcode_state: Some(ScalarFixture::Text("RUNNING")),
                ..Default::default()
            })
        )
        .model,
        None,
    );
}

#[test]
fn report_maps_temperatures_to_snapshot() {
    let report = report_with_print(SnapshotPrintFixture {
        gcode_state: Some(ScalarFixture::Text("RUNNING")),
        nozzle_temper: Some(41),
        nozzle_target_temper: Some(220),
        nozzle_temper2: Some(42),
        nozzle_target_temper2: Some(230),
        bed_temper: Some(60),
        bed_target_temper: Some(65),
        chamber_temper: Some(32),
        ..Default::default()
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
    let report = report_with_print(SnapshotPrintFixture {
        gcode_state: Some(ScalarFixture::Text("RUNNING")),
        device: Some(DeviceFixture {
            bed_temp: Some((65 << 16) | 60),
            ctc: Some(CtcFixture {
                state: 1,
                info: TemperatureInfoFixture {
                    temp: (45 << 16) | 32,
                },
            }),
            extruder: Some(ExtruderFixture {
                state: 0x0012,
                info: vec![
                    ExtruderInfoFixture {
                        id: 0,
                        info: Some(8),
                        temp: (220 << 16) | 27,
                    },
                    ExtruderInfoFixture {
                        id: 1,
                        info: Some(8),
                        temp: (215 << 16) | 22,
                    },
                ],
            }),
            nozzle: Some(NozzleFixture {
                exist: 3,
                info: vec![
                    NozzleInfoFixture {
                        id: 0,
                        diameter: 0.4,
                        kind: "XS01",
                        stat: 0,
                    },
                    NozzleInfoFixture {
                        id: 1,
                        diameter: 0.6,
                        kind: "XS00",
                        stat: 0,
                    },
                ],
            }),
        }),
        ..Default::default()
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
    assert_eq!(
        snapshot.nozzle_temperatures[0].diameter_mm.as_deref(),
        Some("0.6")
    );
    assert_eq!(
        snapshot.nozzle_temperatures[0].nozzle_type.as_deref(),
        Some("Stainless steel")
    );
    assert_eq!(snapshot.nozzle_temperatures[1].label.as_deref(), Some("R"));
    assert_eq!(
        snapshot.nozzle_temperatures[1].current_celsius.as_deref(),
        Some("27")
    );
    assert_eq!(
        snapshot.nozzle_temperatures[1].diameter_mm.as_deref(),
        Some("0.4")
    );
    assert_eq!(
        snapshot.nozzle_temperatures[1].nozzle_type.as_deref(),
        Some("Hardened steel")
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
    let report = report_with_print(SnapshotPrintFixture {
        device: Some(DeviceFixture {
            extruder: Some(ExtruderFixture {
                state: 0x0002,
                info: extruder_temperatures(27, 22),
            }),
            ..Default::default()
        }),
        ..Default::default()
    });

    let snapshot = snapshot_from_report(&endpoint(), &report);

    assert_eq!(snapshot.active_nozzle.as_deref(), Some("R"));
}

#[test]
fn report_ignores_bambu_studio_v2_target_nozzle_for_active_snapshot() {
    let report = report_with_print(SnapshotPrintFixture {
        device: Some(DeviceFixture {
            extruder: Some(ExtruderFixture {
                state: 0x0102,
                info: extruder_temperatures(27, 22),
            }),
            ..Default::default()
        }),
        ..Default::default()
    });

    let snapshot = snapshot_from_report(&endpoint(), &report);

    assert_eq!(snapshot.active_nozzle.as_deref(), Some("R"));
}

#[test]
fn report_state_falls_back_to_print_state() {
    let report = report_with_print(SnapshotPrintFixture {
        state: Some(ScalarFixture::Text("READY")),
        ..Default::default()
    });

    assert_eq!(snapshot_from_report(&endpoint(), &report).state, "READY");
}

#[test]
fn report_state_falls_back_to_root_state() {
    let report = value(SnapshotReportFixture {
        state: Some(ScalarFixture::Text("IDLE")),
        ..Default::default()
    });

    assert_eq!(snapshot_from_report(&endpoint(), &report).state, "IDLE");
}

#[test]
fn report_state_skips_non_string_candidates() {
    let report = report_with_print(SnapshotPrintFixture {
        gcode_state: Some(ScalarFixture::Number(123)),
        state: Some(ScalarFixture::Text("READY")),
        ..Default::default()
    });

    assert_eq!(snapshot_from_report(&endpoint(), &report).state, "READY");
}

#[test]
fn report_state_defaults_to_unknown() {
    let report = report_with_print(SnapshotPrintFixture {
        gcode_state: Some(ScalarFixture::Number(123)),
        ..Default::default()
    });

    assert_eq!(snapshot_from_report(&endpoint(), &report).state, "unknown");
}

#[test]
fn report_name_defaults_to_serial() {
    let mut endpoint = endpoint();
    endpoint.name = None;

    assert_eq!(
        snapshot_from_report(&endpoint, &value(SnapshotReportFixture::default())).name,
        "01S00EXAMPLE"
    );
}
