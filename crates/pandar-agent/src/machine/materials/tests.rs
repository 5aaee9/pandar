use serde_json::{Value, json};

use super::*;

fn normalize(report: Value) -> Option<Value> {
    normalize_material_patch(&report, "2026-06-23T00:00:00Z")
}

#[test]
fn full_ams_snapshot_normalizes_units_trays_external_and_active_tray() {
    let patch = normalize(json!({
        "print": {
            "ams": {
                "tray_now": 5,
                    "ams": [{
                        "id": "1",
                        "humidity": 30,
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
                    "tray_info_idx": "P123",
                    "tray_color": "11223344"
                }
            }
        }
    }))
    .unwrap();

    assert_eq!(patch["type"], "printer_material_patch");
    assert_eq!(patch["ams_units"][0]["replace_trays"], true);
    assert_eq!(patch["ams_units"][0]["unit_kind"], "ams");
    assert_eq!(patch["ams_units"][0]["humidity"], 30);
    assert_eq!(patch["ams_units"][0]["temperature_celsius"], 28);
    assert_eq!(patch["ams_units"][0]["trays"][1]["global_tray_id"], 5);
    assert_eq!(patch["ams_units"][0]["trays"][1]["filament_id"], "GFL05_07");
    assert_eq!(patch["ams_units"][0]["trays"][1]["setting_id"], "GFSL05");
    assert_eq!(patch["ams_units"][0]["trays"][1]["color"], "AABBCC");
    assert_eq!(
        patch["ams_units"][0]["trays"][1]["multi_color"],
        json!(["112233", "445566"])
    );
    assert_eq!(
        patch["ams_units"][0]["trays"][1]["remaining_estimate"],
        "73"
    );
    assert_eq!(patch["external_spools"][0]["external_id"], "254");
    assert_eq!(patch["external_spools"][0]["exists"], true);
    assert_eq!(patch["external_spools"][0]["tray_id"], "0");
    assert_eq!(patch["external_spools"][0]["filament_id"], "P123");
    assert_eq!(patch["external_spools"][0]["color"], "11223344");
    assert!(patch.get("replace_external_spools").is_none());
    assert_eq!(
        patch["active_tray"],
        json!({"kind": "ams", "global_tray_id": 5, "ams_id": "1", "tray_id": "1"})
    );
}

#[test]
fn partial_update_emits_only_observed_material_fields() {
    let patch = normalize(json!({
        "print": {
            "ams": {
                "ams": [{
                    "id": 0,
                    "tray": [{"id": 2, "tray_color": "#00ff11"}]
                }]
            }
        }
    }))
    .unwrap();

    let tray = &patch["ams_units"][0]["trays"][0];
    assert_eq!(tray["tray_id"], "2");
    assert_eq!(tray["color"], "00FF11");
    assert!(tray.get("filament_id").is_none());
    assert!(patch["ams_units"][0].get("replace_trays").is_none());
    assert!(patch.get("active_tray").is_none());
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
        let patch = normalize(json!({
            "print": {"ams": {
                "tray_exist_bits": bits,
                "ams": [{"id": 0, "tray": [{"id": 0}, {"id": 2}]}]
            }}
        }))
        .unwrap();

        let trays = patch["ams_units"][0]["trays"].as_array().unwrap();
        assert!(
            trays
                .iter()
                .any(|tray| tray["tray_id"] == "1" && tray["exists"] == false)
        );
        assert!(
            trays
                .iter()
                .any(|tray| tray["tray_id"] == "3" && tray["state"] == "9")
        );
        assert!(
            trays
                .iter()
                .any(|tray| tray["tray_id"] == "1" && tray["filament_id"].is_null())
        );
        assert_eq!(patch["ams_units"][0]["replace_trays"], true);
        assert!(
            trays
                .iter()
                .all(|tray| tray.get("tray_exist_bits").is_none())
        );
    }
}

#[test]
fn tray_exist_bits_absent_slots_override_stale_tray_objects() {
    let patch = normalize(json!({
        "print": {"ams": {
            "tray_exist_bits": 1,
            "ams": [{"id": 0, "tray": [
                {"id": 0, "tray_info_idx": "GFL05"},
                {"id": 1, "tray_info_idx": "GFL99", "tray_color": "#ff0000"}
            ]}]
        }}
    }))
    .unwrap();

    let trays = patch["ams_units"][0]["trays"].as_array().unwrap();
    let slot_one = trays.iter().find(|tray| tray["tray_id"] == "1").unwrap();
    assert_eq!(slot_one["exists"], false);
    assert!(slot_one["filament_id"].is_null());
    assert!(slot_one["color"].is_null());
}

#[test]
fn tray_exist_bits_use_global_tray_bits_across_ams_units() {
    let patch = normalize(json!({
        "print": {"ams": {
            "tray_exist_bits": "0x0f",
            "ams": [
                {"id": 0, "tray": [{"id": 0}, {"id": 1}, {"id": 2}, {"id": 3}]},
                {"id": 1, "tray": [{"id": 0, "tray_info_idx": "GFL99"}]}
            ]
        }}
    }))
    .unwrap();

    let unit_zero = &patch["ams_units"][0];
    assert_eq!(unit_zero["replace_trays"], true);
    assert!(
        unit_zero["trays"]
            .as_array()
            .unwrap()
            .iter()
            .all(|tray| tray["exists"] == true)
    );

    let unit_one = &patch["ams_units"][1];
    let trays = unit_one["trays"].as_array().unwrap();
    assert_eq!(unit_one["replace_trays"], true);
    assert_eq!(trays[0]["tray_id"], "0");
    assert_eq!(trays[0]["exists"], false);
    assert!(trays[0]["filament_id"].is_null());
    assert!(trays.iter().any(|tray| tray["tray_id"] == "3"));
}

#[test]
fn power_off_zero_bitmask_skips_clears_but_non_zero_still_cleans_up() {
    let zero = normalize(json!({
        "print": {"ams": {
            "power_on_flag": false,
            "tray_exist_bits": 0,
            "ams": [{"id": 0, "tray": [{"id": 0}]}]
        }}
    }))
    .unwrap();
    assert_eq!(zero["ams_units"][0]["trays"].as_array().unwrap().len(), 1);

    let non_zero = normalize(json!({
        "print": {"ams": {
            "power_on_flag": false,
            "tray_exist_bits": "0x1",
            "ams": [{"id": 0, "tray": [{"id": 0}]}]
        }}
    }))
    .unwrap();
    assert_eq!(
        non_zero["ams_units"][0]["trays"].as_array().unwrap().len(),
        4
    );
}

#[test]
fn replace_external_spools_rules_follow_source_shape() {
    let single_object =
        normalize(json!({"print": {"ams": {"vt_tray": {"tray_type": "PLA"}}}})).unwrap();
    assert!(single_object.get("replace_external_spools").is_none());

    let vir_slot_object =
        normalize(json!({"print": {"ams": {"vir_slot": {"tray_type": "PETG"}}}})).unwrap();
    assert!(vir_slot_object.get("replace_external_spools").is_none());

    let single_array =
        normalize(json!({"print": {"ams": {"vt_tray": [{"tray_type": "PLA"}]}}})).unwrap();
    assert!(single_array.get("replace_external_spools").is_none());

    let multi_array = normalize(
        json!({"print": {"ams": {"vt_tray": [{"tray_type": "PLA"}, {"tray_type": "PETG"}]}}}),
    )
    .unwrap();
    assert_eq!(multi_array["replace_external_spools"], true);
    assert_eq!(multi_array["external_spools"][1]["tray_id"], "1");
}

#[test]
fn vir_slot_takes_precedence_and_single_255_maps_to_external_254() {
    let patch = normalize(json!({
        "print": {"ams": {
            "vt_tray": [{"tray_type": "PLA"}, {"tray_type": "PETG"}],
            "vir_slot": [{"id": 255, "setting_id": "GFSL05_07"}]
        }}
    }))
    .unwrap();

    assert_eq!(patch["replace_external_spools"], true);
    assert_eq!(patch["external_spools"].as_array().unwrap().len(), 1);
    assert_eq!(patch["external_spools"][0]["external_id"], "254");
    assert_eq!(patch["external_spools"][0]["exists"], true);
    assert_eq!(patch["external_spools"][0]["tray_id"], "0");
    assert_eq!(patch["external_spools"][0]["setting_id"], "GFSL05_07");
    assert_eq!(patch["external_spools"][0]["filament_id"], "GFL05");
}

#[test]
fn active_tray_ranges_are_normalized() {
    assert_eq!(
        normalize(json!({"print": {"ams": {"tray_now": 15}}})).unwrap()["active_tray"],
        json!({"kind": "ams", "global_tray_id": 15, "ams_id": "3", "tray_id": "3"})
    );
    assert_eq!(
        normalize(json!({"print": {"ams": {"tray_now": 128}}})).unwrap()["active_tray"],
        json!({"kind": "ams_ht", "global_tray_id": null, "ams_id": "128", "tray_id": "0"})
    );
    assert_eq!(
        normalize(json!({"print": {"ams": {"tray_now": 254}}})).unwrap()["active_tray"],
        json!({"kind": "external", "external_id": "254", "tray_id": "0", "global_tray_id": null})
    );
    assert_eq!(
        normalize(json!({"print": {"ams": {"tray_now": 255}}})).unwrap()["active_tray"],
        Value::Null
    );
}

#[test]
fn ams_ht_unit_has_no_global_tray_id() {
    let patch = normalize(json!({
        "print": {"ams": {"ams": [{"id": 128, "tray": [{"id": 0, "tray_type": "PLA"}]}]}}
    }))
    .unwrap();

    assert_eq!(patch["ams_units"][0]["unit_kind"], "ams_ht");
    assert!(patch["ams_units"][0]["trays"][0]["global_tray_id"].is_null());
}

#[test]
fn color_and_credential_keys_are_filtered() {
    let patch = normalize(json!({
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

    let serialized = serde_json::to_string(&patch).unwrap();
    assert!(!serialized.contains("secret-access"));
    assert!(!serialized.contains("secret-password"));
    assert!(!serialized.contains("secret-passwd"));
    assert!(!serialized.contains("secret-token"));
    assert!(!serialized.contains("secret-auth"));
    assert!(patch["ams_units"][0]["trays"][0].get("color").is_none());
}
