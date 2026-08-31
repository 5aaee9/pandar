use super::*;

#[test]
fn embedded_catalog_contains_supported_abi_series() {
    let catalog = catalog();

    assert_eq!(catalog.default_abi_series, "02.07.01");
    assert_eq!(catalog.abi_series.len(), 7);
    assert_eq!(catalog.abi_series("02.06.00").unwrap().total_exports(), 124);
    assert_eq!(catalog.abi_series("02.08.00").unwrap().total_exports(), 129);
    assert_eq!(catalog.abi_series("02.08.01").unwrap().total_exports(), 130);
    assert_eq!(catalog.abi_series("02.08.02").unwrap().total_exports(), 131);
    assert!(
        !catalog
            .abi_series("02.06.00")
            .unwrap()
            .capabilities
            .filament_cloud
    );
    assert!(
        catalog
            .abi_series("02.06.01")
            .unwrap()
            .capabilities
            .filament_cloud
    );
    assert!(
        !catalog
            .abi_series("02.07.00")
            .unwrap()
            .capabilities
            .print_svc_context
    );
    assert!(
        catalog
            .abi_series("02.07.01")
            .unwrap()
            .capabilities
            .print_svc_context
    );
    assert!(
        !catalog
            .abi_series("02.07.01")
            .unwrap()
            .capabilities
            .bind_model_argument
    );
    assert!(
        catalog
            .abi_series("02.08.00")
            .unwrap()
            .capabilities
            .bind_model_argument
    );
    let studio_2_8_1 = &catalog.abi_series("02.08.01").unwrap().capabilities;
    assert!(studio_2_8_1.print_slicer_uid);
    assert!(studio_2_8_1.ams_sync);
}

#[test]
fn embedded_catalog_matches_reference_snapshots() {
    let expected = [
        (
            "02.06.00",
            "02.06.00.51",
            "b506005bc4ee62124e24bf00e0f58656db3646a6",
            "02.06.00.50",
            103,
        ),
        (
            "02.06.01",
            "02.06.01.55",
            "6eb52d6ac75e32ba2116239c1d756d913053f364",
            "02.06.01.50",
            108,
        ),
        (
            "02.07.00",
            "02.07.00.55",
            "4410c27fb15d57b29fbb1dbebc6edea11a091137",
            "02.06.01.50",
            108,
        ),
        (
            "02.07.01",
            "02.07.01.57",
            "3f126b717ed1f10fee0f32f05ed9731808d0c8bb",
            "02.07.01.51",
            108,
        ),
        (
            "02.08.00",
            "02.08.00.50",
            "a78684a11de4abddad9a6d19eeb75a6a1d2e82a5",
            "02.08.00.53",
            108,
        ),
        (
            "02.08.01",
            "02.08.01.55",
            "ba049f6a2e08c3b6033660bb84da80c08722974b",
            "02.08.01.52",
            109,
        ),
        (
            "02.08.02",
            "02.08.02.61",
            "926a7192574bcb9b3a732e1ec59a46d79cb45466",
            "02.08.02.54",
            110,
        ),
    ];

    for (id, reference_version, commit, agent_version, network_exports) in expected {
        let series = abi_series(id).unwrap();
        assert_eq!(series.reference_studio_version, reference_version);
        assert_eq!(series.studio_commit, commit);
        assert_eq!(series.reference_network_agent_version, agent_version);
        assert_eq!(series.reported_network_agent_version, format!("{id}.99"));
        assert_eq!(series.network_exports, network_exports);
        assert_eq!(series.file_transfer_exports, 21);
        assert_eq!(resolve_studio_version(reference_version).unwrap().id, id);
    }
}

#[test]
fn resolves_four_part_studio_versions_by_first_three_components() {
    assert_eq!(
        resolve_studio_version("02.07.01.62").unwrap().id,
        "02.07.01"
    );
    assert_eq!(resolve_studio_version("2.7.1.62").unwrap().id, "02.07.01");
    assert_eq!(
        resolve_studio_version("02.07.01.99").unwrap().id,
        "02.07.01"
    );
    assert_eq!(
        resolve_studio_version("02.08.01.55").unwrap().id,
        "02.08.01"
    );
    assert!(resolve_studio_version("02.09.00.00").is_err());
    assert!(resolve_studio_version("02.07.01").is_err());
}

#[test]
fn resolves_studio_2_8_2_release_contract() {
    let latest = resolve_studio_version("02.08.02.61").unwrap();

    assert_eq!(latest.id, "02.08.02");
    assert_eq!(
        latest.studio_commit,
        "926a7192574bcb9b3a732e1ec59a46d79cb45466"
    );
    assert_eq!(latest.reference_network_agent_version, "02.08.02.54");
    assert_eq!(latest.reported_network_agent_version, "02.08.02.99");
    assert_eq!(latest.network_exports, 110);
    assert_eq!(latest.file_transfer_exports, 21);
    assert!(latest.capabilities.print_queue_plate_id);
    assert!(latest.capabilities.slot_mappings_sync);
    let previous = abi_series("02.08.01").unwrap();
    assert!(!previous.capabilities.print_queue_plate_id);
    assert!(!previous.capabilities.slot_mappings_sync);
    assert_eq!(
        latest.native_modes(),
        ["version", "bind", "print", "ams", "slot-mappings", "ft"]
    );
}

#[test]
fn selects_native_modes_by_abi_capabilities() {
    assert_eq!(
        abi_series("02.08.00").unwrap().native_modes(),
        ["version", "bind", "print", "ft"]
    );
    assert_eq!(
        abi_series("02.08.01").unwrap().native_modes(),
        ["version", "bind", "print", "ams", "ft"]
    );
}

#[test]
fn selects_release_assets_by_abi_series() {
    assert_eq!(
        abi_series("02.07.01").unwrap().hook_bundle_name(),
        "pandar-studio-hook-02.07.01-windows-amd64.zip"
    );
    assert_eq!(
        abi_series("02.08.00").unwrap().hook_bundle_name(),
        "pandar-studio-hook-02.08.00-windows-amd64.zip"
    );
    assert_eq!(
        abi_series("02.08.01").unwrap().hook_bundle_name(),
        "pandar-studio-hook-02.08.01-windows-amd64.zip"
    );
    assert_eq!(
        abi_series("02.08.02").unwrap().hook_bundle_name(),
        "pandar-studio-hook-02.08.02-windows-amd64.zip"
    );
}

#[test]
fn rejects_unknown_or_duplicate_abi_series() {
    assert!(catalog().abi_series("02.09.00").is_err());
    let duplicate = ABI_SERIES_MANIFEST.replace("\"02.08.00\"", "\"02.07.01\"");
    assert!(StudioAbiSeriesCatalog::parse(&duplicate).is_err());
}
