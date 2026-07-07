use serde::Serialize;
use serde_json::Value;

mod types;

pub(super) use types::*;

#[derive(Serialize)]
struct MaterialReport<'a> {
    print: MaterialPrint<'a>,
}

#[derive(Default, Serialize)]
struct MaterialPrint<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    nozzle_temper: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nozzle_temper2: Option<u32>,
    ams: MaterialAms<'a>,
}

#[derive(Default, Serialize)]
struct MaterialAms<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    tray_now: Option<Scalar<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tray_exist_bits: Option<Scalar<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    power_on_flag: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    ams: Vec<MaterialAmsUnit<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vt_tray: Option<ExternalTray<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    vir_slot: Vec<ExternalTray<'a>>,
}

#[derive(Serialize)]
struct MaterialAmsUnit<'a> {
    id: Scalar<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    info: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    humidity: Option<Scalar<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    humidity_raw: Option<Scalar<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temp: Option<&'a str>,
    tray: Vec<MaterialTray<'a>>,
}

#[derive(Default, Serialize)]
struct MaterialTray<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Scalar<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tray_info_idx: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tray_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tray_color: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tray_sub_brands: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag_uid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tray_uuid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    k: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remain: Option<Scalar<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    cols: Vec<&'a str>,
}

#[derive(Default, Serialize)]
struct ExternalTray<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Scalar<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extruder_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tray_info_idx: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tray_color: Option<&'a str>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(untagged)]
enum Scalar<'a> {
    Str(&'a str),
    U32(u32),
}

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
                        },
                        tray_with_id(Scalar::Str("2")),
                        tray_with_id(Scalar::Str("3")),
                    ],
                }],
                vt_tray: Some(ExternalTray {
                    id: Some(Scalar::U32(254)),
                    extruder_id: Some(0),
                    tray_info_idx: Some("P123"),
                    tray_color: Some("11223344"),
                }),
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
    ams_unit_pair_report(Some(28), Some(27), Vec::new())
}

pub(super) fn dual_external_slot_ams_report() -> Value {
    ams_unit_pair_report(
        None,
        None,
        vec![
            ExternalTray {
                id: Some(Scalar::U32(254)),
                ..Default::default()
            },
            ExternalTray {
                id: Some(Scalar::U32(255)),
                ..Default::default()
            },
        ],
    )
}

pub(super) fn single_nozzle_ams_report() -> Value {
    ams_unit_pair_report(Some(28), None, Vec::new())
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
    vir_slot: Vec<ExternalTray<'_>>,
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

fn value(input: impl Serialize) -> Value {
    serde_json::to_value(input).unwrap()
}
