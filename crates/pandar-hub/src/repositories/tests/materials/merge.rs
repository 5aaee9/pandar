use super::fixtures::*;

#[tokio::test]
async fn partial_replay_merges_absent_null_and_concrete_fields() {
    let (materials, tenant, agent, printer_id) = fixture().await;

    materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            MaterialPatchFixture {
                ams_units: Some(vec![PatchAmsUnit {
                    humidity: Some(Some(30.0)),
                    ..ams_unit(
                        "0",
                        vec![
                            tray("0", "0", "PLA", "FF0000"),
                            tray("0", "1", "PETG", "00FF00"),
                        ],
                    )
                }]),
                external_spools: Some(vec![external_spool("254", "0", Some(Some("PLA")))]),
                active_tray: Some(Some(active_ams_tray("0", "0"))),
                ..material_patch("2026-06-23T00:00:00Z")
            },
        ))
        .await
        .unwrap();

    let merged = materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            MaterialPatchFixture {
                ams_units: Some(vec![PatchAmsUnit {
                    humidity: Some(None),
                    ..ams_unit(
                        "0",
                        vec![tray_without_unit("1", Some(Some("ABS")), Some(None))],
                    )
                }]),
                active_tray: Some(None),
                ..material_patch("2026-06-23T00:00:00Z")
            },
        ))
        .await
        .unwrap()
        .unwrap();

    let units = ams_units(&merged);
    let unit = &units[0];
    assert_eq!(unit.humidity, None);
    assert_eq!(unit.trays[0].material_type.as_deref(), Some("PLA"));
    assert_eq!(unit.trays[1].material_type.as_deref(), Some("ABS"));
    assert_eq!(unit.trays[1].color, None);
    assert_eq!(
        external_spools(&merged)[0].material_type.as_deref(),
        Some("PLA")
    );
    assert!(merged.active_tray.is_none());
}

#[tokio::test]
async fn first_snapshot_and_new_entries_drop_null_fields() {
    let (materials, tenant, agent, printer_id) = fixture().await;

    let created = materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            MaterialPatchFixture {
                ams_units: Some(vec![PatchAmsUnit {
                    humidity: Some(None),
                    ..ams_unit(
                        "0",
                        vec![tray_without_unit("0", Some(None), Some(Some("FF0000")))],
                    )
                }]),
                external_spools: Some(vec![external_spool("254", "0", Some(None))]),
                ..material_patch("2026-06-23T00:00:00Z")
            },
        ))
        .await
        .unwrap()
        .unwrap();
    let created_units = ams_units(&created);
    let created_external_spools = external_spools(&created);
    assert_eq!(created_units[0].humidity, None);
    assert_eq!(created_units[0].trays[0].material_type, None);
    assert_eq!(created_units[0].trays[0].color.as_deref(), Some("FF0000"));
    assert_eq!(created_external_spools[0].material_type, None);

    let merged = materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            MaterialPatchFixture {
                ams_units: Some(vec![ams_unit(
                    "0",
                    vec![tray_without_unit("1", Some(None), Some(Some("00FF00")))],
                )]),
                external_spools: Some(vec![external_spool("254", "1", Some(None))]),
                ..material_patch("2026-06-23T00:00:00Z")
            },
        ))
        .await
        .unwrap()
        .unwrap();
    let merged_units = ams_units(&merged);
    let merged_external_spools = external_spools(&merged);
    assert_eq!(merged_units[0].trays[1].material_type, None);
    assert_eq!(merged_units[0].trays[1].color.as_deref(), Some("00FF00"));
    assert_eq!(merged_external_spools[1].material_type, None);
}

#[tokio::test]
async fn replacement_flags_remove_unmentioned_collections() {
    let (materials, tenant, agent, printer_id) = fixture().await;

    materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            MaterialPatchFixture {
                ams_units: Some(vec![ams_unit(
                    "0",
                    vec![
                        tray("0", "0", "PLA", "FF0000"),
                        tray("0", "1", "PETG", "00FF00"),
                    ],
                )]),
                external_spools: Some(vec![
                    external_spool("254", "0", None),
                    external_spool("254", "1", None),
                ]),
                ..material_patch("2026-06-23T00:00:00Z")
            },
        ))
        .await
        .unwrap();
    let replaced = materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            MaterialPatchFixture {
                ams_units: Some(vec![PatchAmsUnit {
                    replace_trays: Some(true),
                    ..ams_unit("0", vec![tray("0", "1", "ABS", "0000FF")])
                }]),
                replace_external_spools: Some(true),
                external_spools: Some(vec![external_spool("254", "1", None)]),
                ..material_patch("2026-06-23T00:01:00Z")
            },
        ))
        .await
        .unwrap()
        .unwrap();

    let replaced_units = ams_units(&replaced);
    let replaced_external_spools = external_spools(&replaced);
    assert_eq!(replaced_units[0].trays.len(), 1);
    assert_eq!(replaced_units[0].trays[0].tray_id, "1");
    assert_eq!(replaced_external_spools.len(), 1);
    assert_eq!(replaced_external_spools[0].tray_id, "1");
}

#[tokio::test]
async fn credential_shaped_keys_and_values_are_not_persisted() {
    let (materials, tenant, agent, printer_id) = fixture().await;

    let snapshot = materials
        .upsert_from_patch(patch_input(
            tenant.id,
            agent.id,
            &printer_id,
            MaterialPatchFixture {
                ams_units: Some(vec![PatchAmsUnit {
                    access_code: Some("secret-code"),
                    ..ams_unit(
                        "0",
                        vec![PatchTray {
                            password: Some("secret-password"),
                            name: Some("token-secret"),
                            ..tray_without_unit("0", Some(Some("PLA")), None)
                        }],
                    )
                }]),
                external_spools: Some(vec![PatchExternalSpool {
                    auth: Some("secret-auth"),
                    ..external_spool("254", "0", None)
                }]),
                active_tray: Some(Some(PatchActiveTray {
                    kind: "ams",
                    ams_id: None,
                    tray_id: "0",
                    token: Some("secret-token"),
                })),
                ..material_patch("2026-06-23T00:00:00Z")
            },
        ))
        .await
        .unwrap()
        .unwrap();

    let persisted = snapshot.persisted_json();
    for needle in ["access_code", "password", "auth", "token", "secret"] {
        assert!(!persisted.contains(needle), "persisted sensitive {needle}");
    }
    assert_eq!(
        ams_units(&snapshot)[0].trays[0].material_type.as_deref(),
        Some("PLA")
    );
}
