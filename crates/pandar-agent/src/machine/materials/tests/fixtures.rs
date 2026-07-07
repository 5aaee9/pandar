use serde::{Deserialize, Serialize};
use serde_json::Value;

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

#[derive(Debug, Deserialize, PartialEq)]
pub(super) struct TestMaterialPatch {
    #[serde(rename = "type")]
    pub(super) document_type: String,
    #[serde(default)]
    pub(super) ams_units: Vec<TestAmsUnit>,
    pub(super) external_spools: Option<Vec<TestExternalSpool>>,
    pub(super) replace_external_spools: Option<bool>,
    pub(super) active_tray: Option<TestActiveTray>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub(super) struct TestAmsUnit {
    pub(super) unit_id: String,
    pub(super) unit_kind: String,
    #[serde(default)]
    pub(super) trays: Vec<TestAmsTray>,
    pub(super) replace_trays: Option<bool>,
    pub(super) humidity: Option<f64>,
    pub(super) humidity_level: Option<f64>,
    pub(super) temperature_celsius: Option<f64>,
    pub(super) toolhead: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub(super) struct TestAmsTray {
    pub(super) tray_id: String,
    pub(super) exists: Option<bool>,
    pub(super) global_tray_id: Option<u64>,
    pub(super) filament_id: Option<String>,
    pub(super) setting_id: Option<String>,
    #[serde(rename = "type")]
    pub(super) material_type: Option<String>,
    pub(super) name: Option<String>,
    pub(super) color: Option<String>,
    pub(super) multi_color: Option<Vec<String>>,
    pub(super) remaining_estimate: Option<String>,
    pub(super) k_value: Option<String>,
    pub(super) state: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub(super) struct TestExternalSpool {
    pub(super) external_id: String,
    pub(super) exists: Option<bool>,
    pub(super) tray_id: String,
    pub(super) setting_id: Option<String>,
    pub(super) filament_id: Option<String>,
    #[serde(rename = "type")]
    pub(super) material_type: Option<String>,
    pub(super) name: Option<String>,
    pub(super) color: Option<String>,
    pub(super) remaining_estimate: Option<String>,
    pub(super) toolhead: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum TestActiveTray {
    Ams {
        global_tray_id: i64,
        ams_id: String,
        tray_id: String,
    },
    AmsHt {
        global_tray_id: Option<u64>,
        ams_id: String,
        tray_id: String,
    },
    External {
        external_id: String,
        tray_id: String,
        global_tray_id: Option<u64>,
    },
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
