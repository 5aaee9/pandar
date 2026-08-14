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
        snapshot_from_json(report),
        MachineSnapshot {
            serial: "01S00EXAMPLE".to_string(),
            host: Some("192.0.2.10".to_string()),
            access_code: Some("12345678".to_string()),
            name: "garage-a1".to_string(),
            model: Some("A1 Mini".to_string()),
            state: Some("RUNNING".to_string()),
            nozzle_temperatures: Vec::new(),
            active_nozzle: None,
            bed_temperature_celsius: None,
            bed_target_temperature_celsius: None,
            chamber_temperature_celsius: None,
            chamber_target_temperature_celsius: None,
            chamber_light_on: None,
            cooling_system: None,
            device_features: None,
            device_features2: None,
            nozzle_system: None,
            telemetry_authoritative: false,
        }
    );
}

#[test]
fn report_maps_legacy_fan_levels_to_bambu_studio_percentages() {
    let report = report_with_print(SnapshotPrintFixture {
        cooling_fan_speed: Some(15),
        big_fan1_speed: Some(9),
        big_fan2_speed: Some(0),
        ..Default::default()
    });

    let cooling = snapshot_from_json(report).cooling_system.unwrap();

    assert_eq!(cooling.mode, None);
    assert_eq!(
        cooling.fans,
        vec![
            pandar_core::PrinterCoolingFan {
                kind: pandar_core::PrinterCoolingFanKind::PartCooling,
                speed_percent: 100,
            },
            pandar_core::PrinterCoolingFan {
                kind: pandar_core::PrinterCoolingFanKind::Auxiliary,
                speed_percent: 60,
            },
            pandar_core::PrinterCoolingFan {
                kind: pandar_core::PrinterCoolingFanKind::Chamber,
                speed_percent: 0,
            },
        ]
    );
}

#[test]
fn report_maps_modern_airduct_mode_and_parts() {
    let report = report_with_print(SnapshotPrintFixture {
        cooling_fan_speed: Some(15),
        device: Some(DeviceFixture {
            airduct: Some(AirDuctFixture {
                mode: 1,
                parts: vec![
                    AirDuctPartFixture { id: 16, state: 80 },
                    AirDuctPartFixture { id: 48, state: 40 },
                    AirDuctPartFixture { id: 80, state: 100 },
                ],
            }),
            ..Default::default()
        }),
        ..Default::default()
    });

    let cooling = snapshot_from_json(report).cooling_system.unwrap();

    assert_eq!(cooling.mode, Some(pandar_core::PrinterCoolingMode::Heating));
    assert_eq!(
        cooling.fans,
        vec![
            pandar_core::PrinterCoolingFan {
                kind: pandar_core::PrinterCoolingFanKind::PartCooling,
                speed_percent: 80,
            },
            pandar_core::PrinterCoolingFan {
                kind: pandar_core::PrinterCoolingFanKind::Chamber,
                speed_percent: 40,
            },
            pandar_core::PrinterCoolingFan {
                kind: pandar_core::PrinterCoolingFanKind::Controller,
                speed_percent: 100,
            },
        ]
    );
}

#[test]
fn report_maps_device_features_to_full_snapshot() {
    let report = report_with_print(SnapshotPrintFixture {
        fun: Some("8000004100000020"),
        ..Default::default()
    });

    assert_eq!(
        snapshot_from_json(report)
            .device_features
            .expect("valid print.fun maps to the full snapshot")
            .bits(),
        0x8000_0041_0000_0020
    );
}

#[test]
fn report_preserves_secondary_device_features_without_interpreting_them() {
    let report = report_with_print(SnapshotPrintFixture {
        fun2: Some("8000000000000021"),
        ..Default::default()
    });

    assert_eq!(
        snapshot_from_json(report).device_features2.unwrap().bits(),
        0x8000_0000_0000_0021
    );
}

#[test]
fn report_maps_only_known_chamber_light_modes_to_snapshot() {
    for (mode, expected) in [
        ("on", Some(true)),
        ("flashing", Some(true)),
        ("off", Some(false)),
        ("future-mode", None),
    ] {
        let report = report_with_print(SnapshotPrintFixture {
            lights_report: vec![LightReportFixture {
                node: "chamber_light",
                mode,
            }],
            ..Default::default()
        });

        assert_eq!(
            snapshot_from_json(report).chamber_light_on,
            expected,
            "mode {mode}"
        );
    }
}

#[test]
fn report_maps_to_snapshot_without_configured_model() {
    let mut endpoint = endpoint();
    endpoint.model = None;

    assert_eq!(
        snapshot_from_json_for(
            &endpoint,
            report_with_print(SnapshotPrintFixture {
                gcode_state: Some(ScalarFixture::Text("RUNNING")),
                ..Default::default()
            }),
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
        ctt: Some(45),
        ..Default::default()
    });

    let snapshot = snapshot_from_json(report);

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
    assert_eq!(
        snapshot.chamber_target_temperature_celsius.as_deref(),
        Some("45")
    );
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
                        snow: None,
                        hnow: None,
                    },
                    ExtruderInfoFixture {
                        id: 1,
                        info: Some(8),
                        temp: (215 << 16) | 22,
                        snow: None,
                        hnow: None,
                    },
                ],
            }),
            nozzle: Some(NozzleFixture {
                exist: 3,
                state: None,
                src_id: None,
                tar_id: None,
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
                        kind: "XH00",
                        stat: 0,
                    },
                ],
            }),
            holder: None,
            airduct: None,
        }),
        ..Default::default()
    });

    let snapshot = snapshot_from_json(report);

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
        Some("XH00")
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
        Some("XS01")
    );
    assert_eq!(snapshot.active_nozzle.as_deref(), Some("L"));
    assert_eq!(snapshot.bed_temperature_celsius.as_deref(), Some("60"));
    assert_eq!(
        snapshot.bed_target_temperature_celsius.as_deref(),
        Some("65")
    );
    assert_eq!(snapshot.chamber_temperature_celsius.as_deref(), Some("32"));
    assert_eq!(
        snapshot.chamber_target_temperature_celsius.as_deref(),
        Some("45")
    );
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

    let snapshot = snapshot_from_json(report);

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

    let snapshot = snapshot_from_json(report);

    assert_eq!(snapshot.active_nozzle.as_deref(), Some("R"));
}

#[test]
fn report_maps_h2c_rack_holder_and_extruder_routing() {
    let report = report_with_print(SnapshotPrintFixture {
        device: Some(DeviceFixture {
            extruder: Some(ExtruderFixture {
                state: 1,
                info: vec![ExtruderInfoFixture {
                    id: 0,
                    info: Some(8),
                    temp: (220 << 16) | 27,
                    snow: Some(16),
                    hnow: Some(16),
                }],
            }),
            nozzle: Some(NozzleFixture {
                exist: 1 << 16,
                state: Some(0),
                src_id: Some(16),
                tar_id: Some(17),
                info: vec![NozzleInfoFixture {
                    id: 16,
                    diameter: 0.4,
                    kind: "XS01",
                    stat: 0,
                }],
            }),
            holder: Some(HolderFixture {
                stat: 0,
                pos: 2,
                info: 0,
            }),
            ..Default::default()
        }),
        ..Default::default()
    });

    let snapshot = snapshot_from_json(report);
    let system = snapshot.nozzle_system.unwrap();
    assert_eq!(system.nozzle.exist, Some(1 << 16));
    assert_eq!(system.nozzle.src_id, Some(16));
    assert_eq!(system.nozzle.tar_id, Some(17));
    assert_eq!(system.nozzle.info[0].id, 16);
    assert_eq!(system.holder.unwrap().pos, Some(2));
    assert_eq!(snapshot.nozzle_temperatures[0].snow, Some(16));
    assert_eq!(snapshot.nozzle_temperatures[0].hnow, Some(16));
}

#[test]
fn report_state_falls_back_to_print_state() {
    let report = report_with_print(SnapshotPrintFixture {
        state: Some(ScalarFixture::Text("READY")),
        ..Default::default()
    });

    assert_eq!(snapshot_from_json(report).state.as_deref(), Some("READY"));
}

#[test]
fn report_state_falls_back_to_root_state() {
    let report = value(SnapshotReportFixture {
        state: Some(ScalarFixture::Text("IDLE")),
        ..Default::default()
    });

    assert_eq!(snapshot_from_json(report).state.as_deref(), Some("IDLE"));
}

#[test]
fn report_state_skips_non_string_candidates() {
    let report = report_with_print(SnapshotPrintFixture {
        gcode_state: Some(ScalarFixture::Number(123)),
        state: Some(ScalarFixture::Text("READY")),
        ..Default::default()
    });

    assert_eq!(snapshot_from_json(report).state.as_deref(), Some("READY"));
}

#[test]
fn report_with_unusable_state_keeps_state_absent() {
    let report = report_with_print(SnapshotPrintFixture {
        gcode_state: Some(ScalarFixture::Number(123)),
        ..Default::default()
    });

    assert_eq!(snapshot_from_json(report).state, None);
}

#[test]
fn report_name_defaults_to_serial() {
    let mut endpoint = endpoint();
    endpoint.name = None;

    assert_eq!(
        snapshot_from_json_for(&endpoint, value(SnapshotReportFixture::default())).name,
        "01S00EXAMPLE"
    );
}
