use serde::Deserialize;
use serde_json::{Value, json};

use super::*;

fn normalize(report: Value) -> Option<Value> {
    normalize_material_patch(&report, "2026-06-23T00:00:00Z")
}

fn material_patch(value: Value) -> TestMaterialPatch {
    serde_json::from_value(value).unwrap()
}

#[test]
fn full_ams_snapshot_normalizes_units_trays_external_and_active_tray() {
    let patch = material_patch(
        normalize(json!({
            "print": {
                "ams": {
                    "tray_now": 5,
                        "ams": [{
                            "id": "1",
                            "info": "10001103",
                            "humidity": 4,
                            "humidity_raw": 30,
                            "temp": "28",
                            "tray": [{
                                "id": "0"
                            }, {
                                "id": "1",
                                "state": 1,
                                "tray_info_idx": "GFL05_07",
                            "tray_type": "PLA",
                            "tray_color": "#aabbcc",
                            "tray_sub_brands": "Basic",
                            "tag_uid": "tag-1",
                            "tray_uuid": "uuid-1",
                                "k": "0.020",
                                "remain": 73,
                                "cols": ["#112233", "not-a-color", "445566"]
                            }, {
                                "id": "2"
                            }, {
                                "id": "3"
                            }]
                        }],
                    "vt_tray": {
                        "id": 254,
                        "extruder_id": 0,
                        "tray_info_idx": "P123",
                        "tray_color": "11223344"
                    }
                }
            }
        }))
        .unwrap(),
    );

    assert_eq!(patch.document_type, "printer_material_patch");
    let unit = &patch.ams_units[0];
    assert_eq!(unit.replace_trays, Some(true));
    assert_eq!(unit.unit_kind, "ams");
    assert_eq!(unit.humidity, Some(30.0));
    assert_eq!(unit.humidity_level, Some(4.0));
    assert_eq!(unit.temperature_celsius, Some(28.0));
    assert_eq!(unit.toolhead.as_deref(), Some("L"));
    let tray = &unit.trays[1];
    assert_eq!(tray.global_tray_id, Some(5));
    assert_eq!(tray.filament_id.as_deref(), Some("GFL05_07"));
    assert_eq!(tray.setting_id.as_deref(), Some("GFSL05"));
    assert_eq!(tray.k_value.as_deref(), Some("0.020"));
    assert_eq!(tray.color.as_deref(), Some("AABBCC"));
    assert_eq!(
        tray.multi_color.as_deref(),
        Some(&["112233".to_owned(), "445566".to_owned()][..])
    );
    assert_eq!(tray.remaining_estimate.as_deref(), Some("73"));

    let external = &patch.external_spools.as_ref().unwrap()[0];
    assert_eq!(external.external_id, "255");
    assert_eq!(external.exists, Some(true));
    assert_eq!(external.tray_id, "0");
    assert_eq!(external.filament_id.as_deref(), Some("P123"));
    assert_eq!(external.color.as_deref(), Some("11223344"));
    assert_eq!(external.toolhead.as_deref(), Some("R"));
    assert_eq!(patch.replace_external_spools, None);
    assert_eq!(
        patch.active_tray,
        Some(TestActiveTray::Ams {
            global_tray_id: 5,
            ams_id: "1".to_owned(),
            tray_id: "1".to_owned(),
        })
    );
}

#[test]
fn humidity_raw_is_normalized_as_percent_and_humidity_as_level() {
    let patch = material_patch(
        normalize(json!({
            "print": {
                "ams": {
                    "ams": [{
                        "id": "0",
                        "humidity": "4",
                        "humidity_raw": "24",
                        "tray": [{"id": "0"}]
                    }]
                }
            }
        }))
        .unwrap(),
    );

    assert_eq!(patch.ams_units[0].humidity, Some(24.0));
    assert_eq!(patch.ams_units[0].humidity_level, Some(4.0));
}

#[test]
fn dual_nozzle_report_defaults_two_ams_units_to_right_and_left_toolheads() {
    let patch = material_patch(
        normalize(json!({
            "print": {
                "nozzle_temper": 28,
                "nozzle_temper2": 27,
                "ams": {
                    "ams": [
                        {"id": "0", "tray": [{"id": "0"}]},
                        {"id": "1", "tray": [{"id": "0"}]}
                    ]
                }
            }
        }))
        .unwrap(),
    );

    assert_eq!(patch.ams_units[0].toolhead.as_deref(), Some("R"));
    assert_eq!(patch.ams_units[1].toolhead.as_deref(), Some("L"));
}

#[test]
fn dual_external_slots_default_two_ams_units_to_right_and_left_toolheads() {
    let patch = material_patch(
        normalize(json!({
            "print": {
                "ams": {
                    "vir_slot": [
                        {"id": 254},
                        {"id": 255}
                    ],
                    "ams": [
                        {"id": "0", "tray": [{"id": "0"}]},
                        {"id": "1", "tray": [{"id": "0"}]}
                    ]
                }
            }
        }))
        .unwrap(),
    );

    assert_eq!(patch.ams_units[0].toolhead.as_deref(), Some("R"));
    assert_eq!(patch.ams_units[1].toolhead.as_deref(), Some("L"));
}

#[test]
fn single_nozzle_report_does_not_guess_ams_toolhead_without_info() {
    let patch = material_patch(
        normalize(json!({
            "print": {
                "nozzle_temper": 28,
                "ams": {
                    "ams": [
                        {"id": "0", "tray": [{"id": "0"}]},
                        {"id": "1", "tray": [{"id": "0"}]}
                    ]
                }
            }
        }))
        .unwrap(),
    );

    assert_eq!(patch.ams_units[0].toolhead, None);
    assert_eq!(patch.ams_units[1].toolhead, None);
}

#[test]
fn decimal_ams_temperature_is_normalized() {
    let patch = material_patch(
        normalize(json!({
            "print": {
                "ams": {
                    "ams": [{
                        "id": "0",
                        "temp": "24.0",
                        "tray": [{"id": "0"}]
                    }]
                }
            }
        }))
        .unwrap(),
    );

    assert_eq!(patch.ams_units[0].temperature_celsius, Some(24.0));
}

#[test]
fn partial_update_emits_only_observed_material_fields() {
    let patch = material_patch(
        normalize(json!({
            "print": {
                "ams": {
                    "ams": [{
                        "id": 0,
                        "tray": [{"id": 2, "tray_color": "#00ff11"}]
                    }]
                }
            }
        }))
        .unwrap(),
    );

    let unit = &patch.ams_units[0];
    let tray = &unit.trays[0];
    assert_eq!(tray.tray_id, "2");
    assert_eq!(tray.color.as_deref(), Some("00FF11"));
    assert_eq!(tray.filament_id, None);
    assert_eq!(unit.replace_trays, None);
    assert_eq!(patch.active_tray, None);
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestMaterialPatch {
    #[serde(rename = "type")]
    document_type: String,
    #[serde(default)]
    ams_units: Vec<TestAmsUnit>,
    external_spools: Option<Vec<TestExternalSpool>>,
    replace_external_spools: Option<bool>,
    active_tray: Option<TestActiveTray>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestAmsUnit {
    unit_id: String,
    unit_kind: String,
    #[serde(default)]
    trays: Vec<TestAmsTray>,
    replace_trays: Option<bool>,
    humidity: Option<f64>,
    humidity_level: Option<f64>,
    temperature_celsius: Option<f64>,
    toolhead: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestAmsTray {
    tray_id: String,
    exists: Option<bool>,
    global_tray_id: Option<u64>,
    filament_id: Option<String>,
    setting_id: Option<String>,
    #[serde(rename = "type")]
    material_type: Option<String>,
    name: Option<String>,
    color: Option<String>,
    multi_color: Option<Vec<String>>,
    remaining_estimate: Option<String>,
    k_value: Option<String>,
    state: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestExternalSpool {
    external_id: String,
    exists: Option<bool>,
    tray_id: String,
    setting_id: Option<String>,
    filament_id: Option<String>,
    #[serde(rename = "type")]
    material_type: Option<String>,
    name: Option<String>,
    color: Option<String>,
    remaining_estimate: Option<String>,
    toolhead: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TestActiveTray {
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

#[test]
fn absent_or_null_report_materials_emit_no_patch() {
    assert_eq!(normalize(json!({})), None);
    assert_eq!(normalize(json!({"print": {"ams": null}})), None);
    assert_eq!(
        normalize(json!({"print": {"ams": {"ams": [{"id": 0}]}}})),
        None
    );
}

#[test]
fn tray_exist_bits_integer_and_hex_clear_missing_normal_ams_slots() {
    for bits in [json!(5), json!("0x5")] {
        let patch = material_patch(
            normalize(json!({
                "print": {"ams": {
                    "tray_exist_bits": bits,
                    "ams": [{"id": 0, "tray": [{"id": 0}, {"id": 2}]}]
                }}
            }))
            .unwrap(),
        );

        let unit = &patch.ams_units[0];
        let trays = &unit.trays;
        assert!(
            trays
                .iter()
                .any(|tray| tray.tray_id == "1" && tray.exists == Some(false))
        );
        assert!(
            trays
                .iter()
                .any(|tray| tray.tray_id == "3" && tray.state.as_deref() == Some("9"))
        );
        assert!(
            trays
                .iter()
                .any(|tray| tray.tray_id == "1" && tray.filament_id.is_none())
        );
        assert_eq!(unit.replace_trays, Some(true));
    }
}

#[test]
fn tray_exist_bits_absent_slots_override_stale_tray_objects() {
    let patch = material_patch(
        normalize(json!({
            "print": {"ams": {
                "tray_exist_bits": 1,
                "ams": [{"id": 0, "tray": [
                    {"id": 0, "tray_info_idx": "GFL05"},
                    {"id": 1, "tray_info_idx": "GFL99", "tray_color": "#ff0000"}
                ]}]
            }}
        }))
        .unwrap(),
    );

    let slot_one = patch.ams_units[0]
        .trays
        .iter()
        .find(|tray| tray.tray_id == "1")
        .unwrap();
    assert_eq!(slot_one.exists, Some(false));
    assert_eq!(slot_one.filament_id, None);
    assert_eq!(slot_one.color, None);
}

#[test]
fn tray_exist_bits_use_global_tray_bits_across_ams_units() {
    let patch = material_patch(
        normalize(json!({
            "print": {"ams": {
                "tray_exist_bits": "0x0f",
                "ams": [
                    {"id": 0, "tray": [{"id": 0}, {"id": 1}, {"id": 2}, {"id": 3}]},
                    {"id": 1, "tray": [{"id": 0, "tray_info_idx": "GFL99"}]}
                ]
            }}
        }))
        .unwrap(),
    );

    let unit_zero = &patch.ams_units[0];
    assert_eq!(unit_zero.replace_trays, Some(true));
    assert!(unit_zero.trays.iter().all(|tray| tray.exists == Some(true)));

    let unit_one = &patch.ams_units[1];
    let trays = &unit_one.trays;
    assert_eq!(unit_one.replace_trays, Some(true));
    assert_eq!(trays[0].tray_id, "0");
    assert_eq!(trays[0].exists, Some(false));
    assert_eq!(trays[0].filament_id, None);
    assert!(trays.iter().any(|tray| tray.tray_id == "3"));
}

#[test]
fn power_off_zero_bitmask_skips_clears_but_non_zero_still_cleans_up() {
    let zero = material_patch(
        normalize(json!({
            "print": {"ams": {
                "power_on_flag": false,
                "tray_exist_bits": 0,
                "ams": [{"id": 0, "tray": [{"id": 0}]}]
            }}
        }))
        .unwrap(),
    );
    assert_eq!(zero.ams_units[0].trays.len(), 1);

    let non_zero = material_patch(
        normalize(json!({
            "print": {"ams": {
                "power_on_flag": false,
                "tray_exist_bits": "0x1",
                "ams": [{"id": 0, "tray": [{"id": 0}]}]
            }}
        }))
        .unwrap(),
    );
    assert_eq!(non_zero.ams_units[0].trays.len(), 4);
}

#[test]
fn replace_external_spools_rules_follow_source_shape() {
    let single_object = material_patch(
        normalize(json!({"print": {"ams": {"vt_tray": {"tray_type": "PLA"}}}})).unwrap(),
    );
    assert_eq!(single_object.replace_external_spools, None);

    let vir_slot_object = material_patch(
        normalize(json!({"print": {"ams": {"vir_slot": {"tray_type": "PETG"}}}})).unwrap(),
    );
    assert_eq!(vir_slot_object.replace_external_spools, None);

    let single_array = material_patch(
        normalize(json!({"print": {"ams": {"vt_tray": [{"tray_type": "PLA"}]}}})).unwrap(),
    );
    assert_eq!(single_array.replace_external_spools, None);

    let multi_array = material_patch(
        normalize(
            json!({"print": {"ams": {"vt_tray": [{"tray_type": "PLA"}, {"tray_type": "PETG"}]}}}),
        )
        .unwrap(),
    );
    let external_spools = multi_array.external_spools.as_ref().unwrap();
    assert_eq!(multi_array.replace_external_spools, Some(true));
    assert_eq!(external_spools[1].tray_id, "1");
    assert_eq!(external_spools[0].external_id, "255");
    assert_eq!(external_spools[1].external_id, "254");
}

#[test]
fn top_level_vt_tray_normalizes_external_spool() {
    let patch = material_patch(
        normalize(json!({
            "print": {
                "vt_tray": [{
                    "id": 254,
                    "extruder_id": 1,
                    "tray_info_idx": "GFG00",
                    "tray_type": "PETG",
                    "tray_color": "00FF00FF",
                    "tray_sub_brands": "PETG HF",
                    "remain": "64"
                }],
                "ams": {
                    "ams": [{"id": 0, "tray": [{"id": 0}]}]
                }
            }
        }))
        .unwrap(),
    );

    let external = &patch.external_spools.as_ref().unwrap()[0];
    assert_eq!(external.external_id, "254");
    assert_eq!(external.exists, Some(true));
    assert_eq!(external.tray_id, "0");
    assert_eq!(external.filament_id.as_deref(), Some("GFG00"));
    assert_eq!(external.material_type.as_deref(), Some("PETG"));
    assert_eq!(external.name.as_deref(), Some("PETG HF"));
    assert_eq!(external.color.as_deref(), Some("00FF00FF"));
    assert_eq!(external.remaining_estimate.as_deref(), Some("64"));
    assert_eq!(external.toolhead.as_deref(), Some("L"));
}

#[test]
fn vir_slot_takes_precedence_and_preserves_single_255_external_id() {
    let patch = material_patch(
        normalize(json!({
            "print": {"ams": {
                "vt_tray": [{"tray_type": "PLA"}, {"tray_type": "PETG"}],
                "vir_slot": [{"id": 255, "setting_id": "GFSL05_07"}]
            }}
        }))
        .unwrap(),
    );

    let external_spools = patch.external_spools.as_ref().unwrap();
    assert_eq!(patch.replace_external_spools, Some(true));
    assert_eq!(external_spools.len(), 1);
    assert_eq!(external_spools[0].external_id, "255");
    assert_eq!(external_spools[0].exists, Some(true));
    assert_eq!(external_spools[0].tray_id, "0");
    assert_eq!(external_spools[0].setting_id.as_deref(), Some("GFSL05_07"));
    assert_eq!(external_spools[0].filament_id.as_deref(), Some("GFL05"));
}

#[test]
fn active_tray_ranges_are_normalized() {
    assert_eq!(
        material_patch(normalize(json!({"print": {"ams": {"tray_now": 15}}})).unwrap()).active_tray,
        Some(TestActiveTray::Ams {
            global_tray_id: 15,
            ams_id: "3".to_owned(),
            tray_id: "3".to_owned(),
        })
    );
    assert_eq!(
        material_patch(normalize(json!({"print": {"ams": {"tray_now": 128}}})).unwrap())
            .active_tray,
        Some(TestActiveTray::AmsHt {
            global_tray_id: None,
            ams_id: "128".to_owned(),
            tray_id: "0".to_owned(),
        })
    );
    assert_eq!(
        material_patch(normalize(json!({"print": {"ams": {"tray_now": 254}}})).unwrap())
            .active_tray,
        Some(TestActiveTray::External {
            external_id: "254".to_owned(),
            tray_id: "0".to_owned(),
            global_tray_id: None,
        })
    );
    assert_eq!(
        material_patch(normalize(json!({"print": {"ams": {"tray_now": 255}}})).unwrap())
            .active_tray,
        None
    );
}

#[test]
fn ams_ht_unit_has_no_global_tray_id() {
    let patch = material_patch(
        normalize(json!({
            "print": {"ams": {"ams": [{"id": 128, "tray": [{"id": 0, "tray_type": "PLA"}]}]}}
        }))
        .unwrap(),
    );

    assert_eq!(patch.ams_units[0].unit_kind, "ams_ht");
    assert_eq!(patch.ams_units[0].trays[0].global_tray_id, None);
}

#[test]
fn color_and_credential_keys_are_filtered() {
    let patch_value = normalize(json!({
        "print": {"ams": {"ams": [{"id": 0, "tray": [{
            "id": 0,
            "tray_color": "not-a-color",
            "access_code": "secret-access",
            "password": "secret-password",
            "passwd": "secret-passwd",
            "token": "secret-token",
            "auth": "secret-auth"
        }]}]}}
    }))
    .unwrap();

    let serialized = serde_json::to_string(&patch_value).unwrap();
    assert!(!serialized.contains("secret-access"));
    assert!(!serialized.contains("secret-password"));
    assert!(!serialized.contains("secret-passwd"));
    assert!(!serialized.contains("secret-token"));
    assert!(!serialized.contains("secret-auth"));
    let patch = material_patch(patch_value);
    assert_eq!(patch.ams_units[0].trays[0].color, None);
}
