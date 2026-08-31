use super::*;

#[test]
fn mismatched_studio_abi_series_is_rejected_before_installation_changes() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let data_dir = temp.path().join("BambuStudio");
    fs::create_dir_all(&data_dir).expect("create data dir");
    let installed_series = pandar_studio_profile::catalog()
        .abi_series
        .iter()
        .find(|series| series.id != STUDIO_ABI_SERIES)
        .expect("catalog contains another supported ABI series");
    let config_path = data_dir.join("BambuStudio.conf");
    let original_config = format!(
        r#"{{"app":{{"version":"{}","installed_networking":"0"}}}}"#,
        installed_series.reference_studio_version
    );
    fs::write(&config_path, &original_config).expect("write config");
    let plugin_file = temp.path().join("pandar_network_plugin.dll");
    fs::write(&plugin_file, b"plugin bytes").expect("write plugin");
    let source_file = temp.path().join("pandar_bambu_source.dll");
    fs::write(&source_file, b"source bytes").expect("write source companion");

    let error = install_network_plugin(InstallNetworkPluginOptions {
        plugin_file,
        source_file,
        data_dir: Some(data_dir.clone()),
    })
    .unwrap_err();

    let error = format!("{error:#}");
    assert!(error.contains(STUDIO_ABI_SERIES));
    assert!(error.contains(&installed_series.id));
    assert!(!data_dir.join("plugins").exists());
    assert_eq!(fs::read_to_string(config_path).unwrap(), original_config);
}

#[test]
fn installed_studio_build_resolves_by_first_three_version_components() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let data_dir = temp.path().join("BambuStudio");
    fs::create_dir_all(&data_dir).expect("create data dir");
    fs::write(
        data_dir.join("BambuStudio.conf"),
        r#"{"app":{"version":"02.07.01.62"}}"#,
    )
    .expect("write config");

    assert_eq!(
        installed_studio_abi_series(&data_dir).unwrap().id,
        "02.07.01"
    );

    fs::write(
        data_dir.join("BambuStudio.conf"),
        r#"{"app":{"version":"02.08.01.55"}}"#,
    )
    .expect("write 2.8.1 config");
    assert_eq!(
        installed_studio_abi_series(&data_dir).unwrap().id,
        "02.08.01"
    );

    fs::write(
        data_dir.join("BambuStudio.conf"),
        r#"{"app":{"version":"02.08.02.61"}}"#,
    )
    .expect("write 2.8.2 config");
    assert_eq!(
        installed_studio_abi_series(&data_dir).unwrap().id,
        "02.08.02"
    );
}
