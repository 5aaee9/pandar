use super::*;

#[test]
fn standard_ams_lite_preserves_its_studio_type_and_conventional_route() {
    let patch = normalize(serde_json::json!({
        "print": {"ams": {"ams": [{
            "id": 0,
            "info": "00000002",
            "tray": [{"id": 0}]
        }]}}
    }))
    .unwrap();

    assert_eq!(patch.ams_units[0].unit_kind, "ams_lite");
    assert_eq!(patch.ams_units[0].trays[0].global_tray_id, Some(0));
}

#[test]
fn mixed_ams_lite_partial_active_update_preserves_global_route_without_guessing_unit() {
    let patch = normalize(serde_json::json!({
        "print": {"ams": {"tray_now": "24"}}
    }))
    .unwrap();

    assert_eq!(
        patch.active_tray,
        Some(TestActiveTray::Ams {
            global_tray_id: 24,
            ams_id: None,
            tray_id: None,
        })
    );
}

#[test]
fn a2l_mixed_ams_lite_uses_studio_global_tray_ids_and_active_route() {
    let patch = normalize(a2l_mixed_ams_lite_report()).unwrap();
    let unit = &patch.ams_units[0];

    assert_eq!(unit.unit_kind, "ams_lite_mixed");
    assert_eq!(
        unit.trays
            .iter()
            .map(|tray| tray.global_tray_id)
            .collect::<Vec<_>>(),
        vec![Some(24), Some(25), Some(26), Some(27)]
    );
    assert_eq!(
        unit.trays
            .iter()
            .map(|tray| tray.exists)
            .collect::<Vec<_>>(),
        vec![Some(true), Some(false), Some(false), Some(false)]
    );
    assert_eq!(
        patch.active_tray,
        Some(TestActiveTray::Ams {
            global_tray_id: 24,
            ams_id: Some("0".to_owned()),
            tray_id: Some("0".to_owned()),
        })
    );
}
