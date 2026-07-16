use serde_json::Value;

mod edge_cases;
mod input;
mod types;

pub(super) use edge_cases::*;
use input::*;
pub(super) use types::*;

pub(super) fn full_ams_snapshot_report() -> Value {
    value(MaterialReport {
        print: MaterialPrint {
            ams: MaterialAms {
                tray_now: Some(Scalar::U32(5)),
                ams: vec![MaterialAmsUnit {
                    id: Scalar::Str("1"),
                    info: Some("10001103"),
                    humidity: Some(Scalar::U32(4)),
                    humidity_raw: Some(Scalar::U32(30)),
                    temp: Some("28"),
                    tray: vec![
                        tray_with_id(Scalar::Str("0")),
                        MaterialTray {
                            id: Some(Scalar::Str("1")),
                            state: Some(1),
                            tray_info_idx: Some("GFL05_07"),
                            tray_type: Some("PLA"),
                            tray_color: Some("#aabbcc"),
                            tray_sub_brands: Some("Basic"),
                            tag_uid: Some("tag-1"),
                            tray_uuid: Some("uuid-1"),
                            k: Some("0.020"),
                            remain: Some(Scalar::U32(73)),
                            cols: vec!["#112233", "not-a-color", "445566"],
                            ..Default::default()
                        },
                        tray_with_id(Scalar::Str("2")),
                        tray_with_id(Scalar::Str("3")),
                    ],
                }],
                vt_tray: Some(ExternalSource::Object(ExternalTray {
                    id: Some(Scalar::U32(254)),
                    extruder_id: Some(0),
                    tray_info_idx: Some("P123"),
                    tray_color: Some("11223344"),
                    ..Default::default()
                })),
                ..Default::default()
            },
            ..Default::default()
        },
    })
}

pub(super) fn humidity_raw_report() -> Value {
    value(single_unit_report(MaterialAmsUnit {
        id: Scalar::Str("0"),
        humidity: Some(Scalar::Str("4")),
        humidity_raw: Some(Scalar::Str("24")),
        tray: vec![tray_with_id(Scalar::Str("0"))],
        info: None,
        temp: None,
    }))
}

pub(super) fn dual_nozzle_ams_report() -> Value {
    ams_unit_pair_report(Some(28), Some(27), None)
}

pub(super) fn dual_external_slot_ams_report() -> Value {
    ams_unit_pair_report(
        None,
        None,
        Some(ExternalSource::Array(vec![
            ExternalTray {
                id: Some(Scalar::U32(254)),
                ..Default::default()
            },
            ExternalTray {
                id: Some(Scalar::U32(255)),
                ..Default::default()
            },
        ])),
    )
}

pub(super) fn single_nozzle_ams_report() -> Value {
    ams_unit_pair_report(Some(28), None, None)
}

pub(super) fn studio_flags_only_report(
    cfg: Option<&str>,
    aux: Option<&str>,
    stat: Option<&str>,
) -> Value {
    value(MaterialReport {
        print: MaterialPrint {
            cfg,
            aux,
            stat,
            ..Default::default()
        },
    })
}

pub(super) fn invalid_studio_flags_material_report() -> Value {
    let mut report = humidity_raw_report();
    report["print"]["cfg"] = Value::Number(1.into());
    report["print"]["aux"] = Value::Bool(true);
    report["print"]["stat"] = Value::String("10000000000000000".to_owned());

    report
}
pub(super) fn filament_switch_ams_report(aux: Option<&str>) -> Value {
    value(MaterialReport {
        print: MaterialPrint {
            aux,
            nozzle_temper: Some(28),
            nozzle_temper2: Some(27),
            ams: MaterialAms {
                ams: vec![
                    MaterialAmsUnit {
                        id: Scalar::Str("0"),
                        info: Some("00000E00"),
                        tray: vec![tray_with_id(Scalar::Str("0"))],
                        humidity: None,
                        humidity_raw: None,
                        temp: None,
                    },
                    MaterialAmsUnit {
                        id: Scalar::Str("1"),
                        info: Some("01000E00"),
                        tray: vec![tray_with_id(Scalar::Str("0"))],
                        humidity: None,
                        humidity_raw: None,
                        temp: None,
                    },
                ],
                ..Default::default()
            },
            ..Default::default()
        },
    })
}

pub(super) fn filament_switch_invalid_binding_report() -> Value {
    let mut report = filament_switch_ams_report(Some("20000000"));
    report["print"]["ams"]["ams"][0]["info"] = Value::String("02000E00".to_owned());
    report
}

pub(super) fn filament_switch_only_report(aux: &str) -> Value {
    value(MaterialReport {
        print: MaterialPrint {
            aux: Some(aux),
            ..Default::default()
        },
    })
}

pub(super) fn decimal_ams_temperature_report() -> Value {
    value(single_unit_report(MaterialAmsUnit {
        id: Scalar::Str("0"),
        temp: Some("24.0"),
        tray: vec![tray_with_id(Scalar::Str("0"))],
        info: None,
        humidity: None,
        humidity_raw: None,
    }))
}

pub(super) fn partial_material_update_report() -> Value {
    value(single_unit_report(MaterialAmsUnit {
        id: Scalar::U32(0),
        tray: vec![MaterialTray {
            id: Some(Scalar::U32(2)),
            tray_color: Some("#00ff11"),
            ..Default::default()
        }],
        info: None,
        humidity: None,
        humidity_raw: None,
        temp: None,
    }))
}

pub(super) fn tray_exist_bits_integer_report() -> Value {
    tray_exist_bits_report(Scalar::U32(5))
}

pub(super) fn tray_exist_bits_hex_report() -> Value {
    tray_exist_bits_report(Scalar::Str("0x5"))
}

pub(super) fn absent_slots_override_report() -> Value {
    value(MaterialReport {
        print: MaterialPrint {
            ams: MaterialAms {
                tray_exist_bits: Some(Scalar::U32(1)),
                ams: vec![MaterialAmsUnit {
                    id: Scalar::U32(0),
                    tray: vec![
                        MaterialTray {
                            id: Some(Scalar::U32(0)),
                            tray_info_idx: Some("GFL05"),
                            ..Default::default()
                        },
                        MaterialTray {
                            id: Some(Scalar::U32(1)),
                            tray_info_idx: Some("GFL99"),
                            tray_color: Some("#ff0000"),
                            ..Default::default()
                        },
                    ],
                    info: None,
                    humidity: None,
                    humidity_raw: None,
                    temp: None,
                }],
                ..Default::default()
            },
            ..Default::default()
        },
    })
}

pub(super) fn global_tray_bits_report() -> Value {
    value(MaterialReport {
        print: MaterialPrint {
            ams: MaterialAms {
                tray_exist_bits: Some(Scalar::Str("0x0f")),
                ams: vec![
                    MaterialAmsUnit {
                        id: Scalar::U32(0),
                        tray: vec![
                            tray_with_id(Scalar::U32(0)),
                            tray_with_id(Scalar::U32(1)),
                            tray_with_id(Scalar::U32(2)),
                            tray_with_id(Scalar::U32(3)),
                        ],
                        info: None,
                        humidity: None,
                        humidity_raw: None,
                        temp: None,
                    },
                    MaterialAmsUnit {
                        id: Scalar::U32(1),
                        tray: vec![MaterialTray {
                            id: Some(Scalar::U32(0)),
                            tray_info_idx: Some("GFL99"),
                            ..Default::default()
                        }],
                        info: None,
                        humidity: None,
                        humidity_raw: None,
                        temp: None,
                    },
                ],
                ..Default::default()
            },
            ..Default::default()
        },
    })
}

pub(super) fn power_off_zero_bitmask_report() -> Value {
    power_off_report(Scalar::U32(0))
}

pub(super) fn power_off_non_zero_bitmask_report() -> Value {
    power_off_report(Scalar::Str("0x1"))
}

fn tray_exist_bits_report(bits: Scalar<'_>) -> Value {
    value(MaterialReport {
        print: MaterialPrint {
            ams: MaterialAms {
                tray_exist_bits: Some(bits),
                ams: vec![MaterialAmsUnit {
                    id: Scalar::U32(0),
                    tray: vec![tray_with_id(Scalar::U32(0)), tray_with_id(Scalar::U32(2))],
                    info: None,
                    humidity: None,
                    humidity_raw: None,
                    temp: None,
                }],
                ..Default::default()
            },
            ..Default::default()
        },
    })
}

fn power_off_report(bits: Scalar<'_>) -> Value {
    value(MaterialReport {
        print: MaterialPrint {
            ams: MaterialAms {
                power_on_flag: Some(false),
                tray_exist_bits: Some(bits),
                ams: vec![unit_with_tray(Scalar::U32(0), Scalar::U32(0))],
                ..Default::default()
            },
            ..Default::default()
        },
    })
}

fn ams_unit_pair_report(
    nozzle_temper: Option<u32>,
    nozzle_temper2: Option<u32>,
    vir_slot: Option<ExternalSource<'_>>,
) -> Value {
    value(MaterialReport {
        print: MaterialPrint {
            nozzle_temper,
            nozzle_temper2,
            ams: MaterialAms {
                ams: vec![
                    unit_with_tray(Scalar::Str("0"), Scalar::Str("0")),
                    unit_with_tray(Scalar::Str("1"), Scalar::Str("0")),
                ],
                vir_slot,
                ..Default::default()
            },
            ..Default::default()
        },
    })
}

fn single_unit_report(unit: MaterialAmsUnit<'_>) -> MaterialReport<'_> {
    MaterialReport {
        print: MaterialPrint {
            ams: MaterialAms {
                ams: vec![unit],
                ..Default::default()
            },
            ..Default::default()
        },
    }
}

fn unit_with_tray<'a>(unit_id: Scalar<'a>, tray_id: Scalar<'a>) -> MaterialAmsUnit<'a> {
    MaterialAmsUnit {
        id: unit_id,
        tray: vec![tray_with_id(tray_id)],
        info: None,
        humidity: None,
        humidity_raw: None,
        temp: None,
    }
}

fn tray_with_id(id: Scalar<'_>) -> MaterialTray<'_> {
    MaterialTray {
        id: Some(id),
        ..Default::default()
    }
}
