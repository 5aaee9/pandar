use std::path::{Path, PathBuf};

#[test]
fn cpp_shim_contains_no_transport_or_status_policy() {
    let credential_sources = read_sources(&[
        "shim_types.hpp",
        "shim_connection.hpp",
        "shim_state.hpp",
        "shim_printer_cache.hpp",
        "shim_dispatch.hpp",
    ]);
    for forbidden in ["printer_connections", "host_ptr", "access_ptr"] {
        assert_absent(&credential_sources, forbidden);
    }

    let status_sources = read_sources(&[
        "shim_dispatch.hpp",
        "shim_abi_content.hpp",
        "shim_printer_cache.hpp",
    ]);
    for forbidden in [
        r#"R"({"print"#,
        r#"\"wifi_signal\""#,
        r#"\"sdcard\""#,
        r#"\"ipcam_dev\""#,
        r#"\"liveview\""#,
        r#"\"rtsp_url\""#,
        r#"\"support_chamber\""#,
        r#"\"support_mqtt_alive\""#,
        "camera_url_for(",
        "bambu:///",
        "rtsps://",
        "rtsp://",
        "\"C11\"",
        "\"IDLE\"",
    ] {
        assert_absent(&status_sources, forbidden);
    }
}

#[test]
fn cpp_account_and_request_adapters_contain_no_policy() {
    let sources = read_sources(&[
        "shim_profile.hpp",
        "shim_firmware.hpp",
        "shim_no_auth.hpp",
        "shim_abi_content.hpp",
        "shim_abi_operations.hpp",
        "shim_abi_account.hpp",
        "shim_abi_user.hpp",
    ]);
    for forbidden in [
        r#"R"({"#,
        "agent->token.empty() || agent->profile_json.empty()",
        "agent->hub_url != expected_hub",
        "hub_url != agent->hub_url",
        "snapshot.token.empty() || snapshot.printer_id.empty()",
        "user_info.empty() || user_info == \"{}\"",
        "return !a->token.empty()",
        "*http_code = 501",
        "http_code = 501",
        "normalized_dev_id.empty() || printer_id.empty()",
    ] {
        assert_absent(&sources, forbidden);
    }
}

#[test]
fn cpp_shim_contains_no_low_level_session_lifecycle() {
    let sources = read_all_shim_sources();
    for forbidden in [
        "pandar_plugin_create_no_auth_session",
        "pandar_plugin_no_auth_rotation_claim",
        "pandar_plugin_no_auth_retry_arm",
        "pandar_plugin_no_auth_retry_active",
        "pandar_plugin_no_auth_retry_begin",
        "pandar_plugin_no_auth_retry_complete",
        "pandar_plugin_account_stage_revoke",
        "pandar_plugin_account_revoke_pending",
        "pandar_plugin_account_revoke_staged",
        "pandar_plugin_account_logout_action",
        "pandar_plugin_delete_session",
        "pandar_plugin_printer_refresh(",
        "get_printers_with_token_refresh",
        "last_error",
    ] {
        assert_absent(&sources, forbidden);
    }
}

#[test]
fn cpp_shim_contains_no_delivery_or_freshness_decisions() {
    let all_shims = read_all_shim_sources();
    for forbidden in [
        "reserve_printer_refresh_observation",
        "with_printer_refresh_firmware",
        "begin_firmware_observation",
        "FirmwareObservationTicket",
        "PluginCoreFirmwareObservation",
        "pandar_plugin_core_reserve_firmware_observation",
        "pandar_plugin_connection_studio_snapshot_current",
        "pandar_plugin_connection_sync_firmware(",
        "pandar_plugin_studio_request_snapshot_current",
        "agent->token.empty()",
    ] {
        assert_absent(&all_shims, forbidden);
    }

    let connection = read_sources(&["shim_connection.hpp"]);
    for forbidden in [
        "pandar_plugin_studio_claim_delivery(",
        "pandar_plugin_studio_complete_delivery(",
        "pandar_plugin_connection_claim_delivery(",
    ] {
        assert_absent(&connection, forbidden);
    }

    let request_snapshot = read_sources(&["shim_request_snapshot.hpp"]);
    for forbidden in [
        "agent->hub_url != snapshot.hub_url",
        "agent->token != snapshot.token",
        "pandar_plugin_studio_account_request_admitted",
        "pandar_plugin_studio_account_request_current",
    ] {
        assert_absent(&request_snapshot, forbidden);
    }

    let account = read_sources(&["shim_account_transaction.hpp"]);
    assert_absent(&account, "had_login");
}

fn assert_absent(source: &str, forbidden: &str) {
    assert!(
        !source.contains(forbidden),
        "C++ shim contains forbidden policy marker {forbidden}"
    );
}

fn read_sources(names: &[&str]) -> String {
    names
        .iter()
        .map(|name| std::fs::read_to_string(source_root().join(name)).expect("C++ shim source"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn read_all_shim_sources() -> String {
    let mut paths = std::fs::read_dir(source_root())
        .expect("plugin source directory")
        .map(|entry| entry.expect("plugin source entry").path())
        .filter(|path| is_shim_source(path))
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .iter()
        .map(|path| std::fs::read_to_string(path).expect("C++ shim source"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_shim_source(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name == "shim.cpp" || (name.starts_with("shim_") && name.ends_with(".hpp"))
        })
}

fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}
