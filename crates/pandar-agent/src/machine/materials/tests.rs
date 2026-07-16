use serde_json::Value;

use super::*;

mod fixtures;

use fixtures::*;

fn normalize(report: Value) -> Option<TestMaterialPatch> {
    let report = parse_materials_report(&report)?;
    let patch = normalize_material_patch(&report, "2026-06-23T00:00:00Z")?;
    Some(decode_patch(&patch))
}

fn normalize_json(report: Value) -> Option<String> {
    let report = parse_materials_report(&report)?;
    let patch = normalize_material_patch(&report, "2026-06-23T00:00:00Z")?;
    Some(serde_json::to_string(&patch).unwrap())
}

fn decode_patch(input: impl serde::Serialize) -> TestMaterialPatch {
    let value = serde_json::to_value(input).unwrap();
    serde::Deserialize::deserialize(value).unwrap()
}

#[test]
fn full_ams_snapshot_normalizes_units_trays_external_and_active_tray() {
    let patch = normalize(full_ams_snapshot_report()).unwrap();

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
    let patch = normalize(humidity_raw_report()).unwrap();

    assert_eq!(patch.ams_units[0].humidity, Some(24.0));
    assert_eq!(patch.ams_units[0].humidity_level, Some(4.0));
}

#[test]
fn dual_nozzle_report_defaults_two_ams_units_to_right_and_left_toolheads() {
    let patch = normalize(dual_nozzle_ams_report()).unwrap();

    assert_eq!(patch.ams_units[0].toolhead.as_deref(), Some("R"));
    assert_eq!(patch.ams_units[1].toolhead.as_deref(), Some("L"));
}

#[test]
fn dual_external_slots_default_two_ams_units_to_right_and_left_toolheads() {
    let patch = normalize(dual_external_slot_ams_report()).unwrap();

    assert_eq!(patch.ams_units[0].toolhead.as_deref(), Some("R"));
    assert_eq!(patch.ams_units[1].toolhead.as_deref(), Some("L"));
}

#[test]
fn single_nozzle_report_does_not_guess_ams_toolhead_without_info() {
    let patch = normalize(single_nozzle_ams_report()).unwrap();

    assert_eq!(patch.ams_units[0].toolhead, None);
    assert_eq!(patch.ams_units[1].toolhead, None);
}

#[test]
fn installed_filament_switch_marks_bound_ams_units_as_shared() {
    let patch = normalize(filament_switch_ams_report(Some("20000000"))).unwrap();

    assert_eq!(patch.ams_units[0].info.as_deref(), Some("00000E00"));
    assert_eq!(patch.ams_units[1].info.as_deref(), Some("01000E00"));
    assert_eq!(patch.ams_units[0].toolhead.as_deref(), Some("LR"));
    assert_eq!(patch.ams_units[1].toolhead.as_deref(), Some("LR"));
}

#[test]
fn switch_bound_ams_without_install_state_does_not_guess_toolheads() {
    let patch = normalize(filament_switch_ams_report(None)).unwrap();

    assert_eq!(patch.ams_units[0].toolhead, None);
    assert_eq!(patch.ams_units[1].toolhead, None);
}

#[test]
fn switch_bound_ams_rejects_an_invalid_switch_input() {
    let patch = normalize(filament_switch_invalid_binding_report()).unwrap();

    assert_eq!(patch.ams_units[0].toolhead, None);
    assert_eq!(patch.ams_units[1].toolhead.as_deref(), Some("LR"));
}

#[test]
fn aux_only_reports_persist_installed_and_absent_switch_states() {
    let installed = normalize(filament_switch_only_report("20000000")).unwrap();
    let absent = normalize(filament_switch_only_report("00000000")).unwrap();

    assert_eq!(installed.filament_switch_installed, Some(true));
    assert_eq!(absent.filament_switch_installed, Some(false));
    assert!(installed.ams_units.is_empty());
}

#[test]
fn studio_flags_preserve_unknown_bits_and_leading_zero_width() {
    let patch = normalize(studio_flags_only_report(
        Some("0x0000000a"),
        Some("0xA4003001"),
        Some("00000000000000ff"),
    ))
    .unwrap();

    assert_eq!(patch.cfg.as_deref(), Some("0000000A"));
    assert_eq!(patch.aux.as_deref(), Some("A4003001"));
    assert_eq!(patch.stat.as_deref(), Some("00000000000000FF"));
    assert_eq!(patch.filament_switch_installed, Some(true));
    assert!(patch.ams_units.is_empty());
}

#[test]
fn cfg_and_stat_only_reports_emit_patches() {
    let cfg = normalize(studio_flags_only_report(Some("00000001"), None, None)).unwrap();
    let stat = normalize(studio_flags_only_report(None, None, Some("00000002"))).unwrap();

    assert_eq!(cfg.cfg.as_deref(), Some("00000001"));
    assert_eq!(cfg.aux, None);
    assert_eq!(stat.stat.as_deref(), Some("00000002"));
    assert_eq!(stat.filament_switch_installed, None);
}

#[test]
fn invalid_studio_flags_are_omitted_instead_of_becoming_false() {
    let report = invalid_studio_flags_material_report();
    let patch = normalize(report.clone()).unwrap();
    let serialized = normalize_json(report).unwrap();

    assert_eq!(patch.cfg, None);
    assert_eq!(patch.aux, None);
    assert_eq!(patch.stat, None);
    assert_eq!(patch.filament_switch_installed, None);
    assert!(!serialized.contains("\"cfg\""));
    assert!(!serialized.contains("\"aux\""));
    assert!(!serialized.contains("\"stat\""));
    assert!(!serialized.contains("\"filament_switch_installed\""));
}

#[test]
fn explicit_empty_studio_flags_are_preserved_and_clear_switch_state() {
    let patch = normalize(studio_flags_only_report(Some(""), Some(""), Some(""))).unwrap();
    let serialized =
        normalize_json(studio_flags_only_report(Some(""), Some(""), Some(""))).unwrap();

    assert_eq!(patch.cfg.as_deref(), Some(""));
    assert_eq!(patch.aux.as_deref(), Some(""));
    assert_eq!(patch.stat.as_deref(), Some(""));
    assert_eq!(patch.filament_switch_installed, Some(false));
    assert!(serialized.contains(r#""aux":""#));
}

#[test]
fn decimal_ams_temperature_is_normalized() {
    let patch = normalize(decimal_ams_temperature_report()).unwrap();

    assert_eq!(patch.ams_units[0].temperature_celsius, Some(24.0));
}

#[test]
fn partial_update_emits_only_observed_material_fields() {
    let patch = normalize(partial_material_update_report()).unwrap();

    let unit = &patch.ams_units[0];
    let tray = &unit.trays[0];
    assert_eq!(tray.tray_id, "2");
    assert_eq!(tray.color.as_deref(), Some("00FF11"));
    assert_eq!(tray.filament_id, None);
    assert_eq!(unit.replace_trays, None);
    assert_eq!(patch.active_tray, None);
}

#[test]
fn absent_or_null_report_materials_emit_no_patch() {
    assert_eq!(normalize(empty_report()), None);
    assert_eq!(normalize(null_ams_report()), None);
    assert_eq!(normalize(empty_ams_unit_report()), None);
}

#[test]
fn tray_exist_bits_integer_and_hex_clear_missing_normal_ams_slots() {
    for report in [
        tray_exist_bits_integer_report(),
        tray_exist_bits_hex_report(),
    ] {
        let patch = normalize(report).unwrap();

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
    let patch = normalize(absent_slots_override_report()).unwrap();

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
    let patch = normalize(global_tray_bits_report()).unwrap();

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
    let zero = normalize(power_off_zero_bitmask_report()).unwrap();
    assert_eq!(zero.ams_units[0].trays.len(), 1);

    let non_zero = normalize(power_off_non_zero_bitmask_report()).unwrap();
    assert_eq!(non_zero.ams_units[0].trays.len(), 4);
}

#[test]
fn replace_external_spools_rules_follow_source_shape() {
    let single_object = normalize(external_spool_single_object_report()).unwrap();
    assert_eq!(single_object.replace_external_spools, None);

    let vir_slot_object = normalize(vir_slot_single_object_report()).unwrap();
    assert_eq!(vir_slot_object.replace_external_spools, None);

    let single_array = normalize(external_spool_single_array_report()).unwrap();
    assert_eq!(single_array.replace_external_spools, None);

    let multi_array = normalize(external_spool_multi_array_report()).unwrap();
    let external_spools = multi_array.external_spools.as_ref().unwrap();
    assert_eq!(multi_array.replace_external_spools, Some(true));
    assert_eq!(external_spools[1].tray_id, "1");
    assert_eq!(external_spools[0].external_id, "255");
    assert_eq!(external_spools[1].external_id, "254");
}

#[test]
fn top_level_vt_tray_normalizes_external_spool() {
    let patch = normalize(top_level_external_spool_report()).unwrap();

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
    let patch = normalize(vir_slot_precedence_report()).unwrap();

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
        normalize(active_tray_report(15)).unwrap().active_tray,
        Some(TestActiveTray::Ams {
            global_tray_id: 15,
            ams_id: "3".to_owned(),
            tray_id: "3".to_owned(),
        })
    );
    assert_eq!(
        normalize(active_tray_report(128)).unwrap().active_tray,
        Some(TestActiveTray::AmsHt {
            global_tray_id: None,
            ams_id: "128".to_owned(),
            tray_id: "0".to_owned(),
        })
    );
    assert_eq!(
        normalize(active_tray_report(254)).unwrap().active_tray,
        Some(TestActiveTray::External {
            external_id: "254".to_owned(),
            tray_id: "0".to_owned(),
            global_tray_id: None,
        })
    );
    assert_eq!(
        normalize(active_tray_report(255)).unwrap().active_tray,
        None
    );
}

#[test]
fn ams_ht_unit_has_no_global_tray_id() {
    let patch = normalize(ams_ht_unit_report()).unwrap();

    assert_eq!(patch.ams_units[0].unit_kind, "ams_ht");
    assert_eq!(patch.ams_units[0].trays[0].global_tray_id, None);
}

#[test]
fn color_and_credential_keys_are_filtered() {
    let serialized = normalize_json(credential_filter_report()).unwrap();
    assert!(!serialized.contains("secret-access"));
    assert!(!serialized.contains("secret-password"));
    assert!(!serialized.contains("secret-passwd"));
    assert!(!serialized.contains("secret-token"));
    assert!(!serialized.contains("secret-auth"));
    let patch = normalize(credential_filter_report()).unwrap();
    assert_eq!(patch.ams_units[0].trays[0].color, None);
}
