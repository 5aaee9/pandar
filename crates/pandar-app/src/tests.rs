use super::*;

#[test]
fn parses_agent_subcommand_with_agent_options() {
    let agent_id = "00000000-0000-4000-8000-000000000001";
    let tenant_id = "00000000-0000-4000-8000-000000000002";

    let cli = Cli::parse_from([
        "pandar",
        "agent",
        "--hub-grpc-url",
        "http://hub.internal:50051",
        "--agent-name",
        "garage",
        "--agent-id",
        agent_id,
        "--tenant-id",
        tenant_id,
        "--agent-credential",
        "pandar_ac_test",
    ]);

    let Command::Agent(config) = cli.command else {
        panic!("expected agent subcommand");
    };
    assert_eq!(config.hub_grpc_url, "http://hub.internal:50051");
    assert_eq!(config.agent_name, "garage");
    assert_eq!(config.agent_id, agent_id);
    assert_eq!(config.tenant_id, tenant_id);
    assert_eq!(config.agent_credential, "pandar_ac_test");
}

#[test]
fn parses_hub_subcommand() {
    let cli = Cli::parse_from(["pandar", "hub"]);

    assert!(matches!(cli.command, Command::Hub));
}

#[test]
fn defaults_install_network_plugin_files_to_release_artifacts_in_current_directory() {
    let cli = Cli::try_parse_from(["pandar", "install-network-plugin"])
        .expect("parse install-network-plugin without explicit release files");

    let Command::InstallNetworkPlugin {
        plugin_file,
        source_file,
        data_dir,
    } = cli.command
    else {
        panic!("expected install-network-plugin subcommand");
    };
    let expected_plugin_file = if cfg!(target_os = "windows") {
        "pandar_network_plugin.dll"
    } else if cfg!(target_os = "macos") {
        "libpandar_network_plugin.dylib"
    } else {
        "libpandar_network_plugin.so"
    };
    let expected_source_file = if cfg!(target_os = "windows") {
        "pandar_bambu_source.dll"
    } else if cfg!(target_os = "macos") {
        "libpandar_bambu_source.dylib"
    } else {
        "libpandar_bambu_source.so"
    };
    assert_eq!(plugin_file, PathBuf::from(expected_plugin_file));
    assert_eq!(source_file, PathBuf::from(expected_source_file));
    assert_eq!(data_dir, None);
}

#[test]
fn parses_install_network_plugin_subcommand() {
    let cli = Cli::parse_from([
        "pandar",
        "install-network-plugin",
        "--plugin-file",
        "target/release/pandar_network_plugin.dll",
        "--source-file",
        "target/release/pandar_bambu_source.dll",
        "--data-dir",
        "C:/Users/test/AppData/Roaming/BambuStudio",
    ]);

    let Command::InstallNetworkPlugin {
        plugin_file,
        source_file,
        data_dir,
    } = cli.command
    else {
        panic!("expected install-network-plugin subcommand");
    };
    assert_eq!(
        plugin_file,
        PathBuf::from("target/release/pandar_network_plugin.dll")
    );
    assert_eq!(
        source_file,
        PathBuf::from("target/release/pandar_bambu_source.dll")
    );
    assert_eq!(
        data_dir,
        Some(PathBuf::from("C:/Users/test/AppData/Roaming/BambuStudio"))
    );
}

#[test]
fn parses_install_studio_hook_subcommand() {
    let cli = Cli::parse_from([
        "pandar",
        "install-studio-hook",
        "--studio-dir",
        "C:/Program Files/Bambu Studio",
        "--data-dir",
        "C:/Users/test/AppData/Roaming/BambuStudio",
    ]);

    let Command::InstallStudioHook {
        studio_dir,
        data_dir,
    } = cli.command
    else {
        panic!("expected install-studio-hook subcommand");
    };
    assert_eq!(
        studio_dir,
        Some(PathBuf::from("C:/Program Files/Bambu Studio"))
    );
    assert_eq!(
        data_dir,
        Some(PathBuf::from("C:/Users/test/AppData/Roaming/BambuStudio"))
    );
}

#[test]
fn parses_uninstall_studio_hook_subcommand() {
    let cli = Cli::parse_from([
        "pandar",
        "uninstall-studio-hook",
        "--studio-dir",
        "C:/Program Files/Bambu Studio",
        "--data-dir",
        "C:/Users/test/AppData/Roaming/BambuStudio",
    ]);

    let Command::UninstallStudioHook {
        studio_dir,
        data_dir,
    } = cli.command
    else {
        panic!("expected uninstall-studio-hook subcommand");
    };
    assert_eq!(
        studio_dir,
        Some(PathBuf::from("C:/Program Files/Bambu Studio"))
    );
    assert_eq!(
        data_dir,
        Some(PathBuf::from("C:/Users/test/AppData/Roaming/BambuStudio"))
    );
}

#[test]
fn rejects_removed_studio_dev_hook_commands() {
    assert!(Cli::try_parse_from(["pandar", "install-studio-dev-hook"]).is_err());
    assert!(Cli::try_parse_from(["pandar", "uninstall-studio-dev-hook"]).is_err());
}

#[test]
fn installer_json_reports_studio_abi_series() {
    let network = serde_json::to_value(NetworkPluginJson {
        studio_abi_series: "02.07.01".to_owned(),
        plugin_path: PathBuf::from("bambu_networking.dll"),
        source_path: PathBuf::from("BambuSource.dll"),
        config_path: PathBuf::from("BambuStudio.conf"),
    })
    .unwrap();
    assert_eq!(network["studio_abi_series"], "02.07.01");
    assert!(network.get("studio_profile").is_none());

    let hook = serde_json::to_value(StudioHookJson {
        studio_abi_series: "02.08.00".to_owned(),
        studio_dir: PathBuf::from("Bambu Studio"),
        proxy_path: PathBuf::from("swscale-8.dll"),
        original_path: PathBuf::from("swscale8original.dll"),
        plugin_path: PathBuf::from("bambu_networking.dll"),
        source_path: PathBuf::from("BambuSource.dll"),
        config_path: PathBuf::from("BambuStudio.conf"),
        plugin_package_path: PathBuf::from("networking_plugins.zip"),
    })
    .unwrap();
    assert_eq!(hook["studio_abi_series"], "02.08.00");
    assert!(hook.get("studio_profile").is_none());
}

#[test]
fn parses_decrypt_bambu_studio_log_subcommand() {
    let cli = Cli::parse_from([
        "pandar",
        "decrypt-bambu-studio-log",
        "--log-file",
        "studio_enc_cn.log.0",
        "--output-file",
        "studio.log",
    ]);

    let Command::DecryptBambuStudioLog {
        log_file,
        output_file,
    } = cli.command
    else {
        panic!("expected decrypt-bambu-studio-log subcommand");
    };
    assert_eq!(log_file, PathBuf::from("studio_enc_cn.log.0"));
    assert_eq!(output_file, PathBuf::from("studio.log"));
}
