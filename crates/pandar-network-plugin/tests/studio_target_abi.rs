use std::ffi::CStr;

use pandar_network_plugin::{
    PluginHttpResult, STUDIO_ABI_SERIES, pandar_plugin_free_with_capacity,
    pandar_plugin_network_agent_version, pandar_plugin_sync_ams_filaments,
    pandar_plugin_sync_slot_mappings,
};

#[test]
fn network_agent_version_matches_selected_studio_abi_series() {
    let version = unsafe { CStr::from_ptr(pandar_plugin_network_agent_version()) }
        .to_str()
        .unwrap();

    let abi_series = pandar_studio_profile::abi_series(STUDIO_ABI_SERIES).unwrap();
    assert_eq!(version, abi_series.reported_network_agent_version);
}

#[test]
fn ams_sync_returns_the_stable_explicit_unsupported_contract() {
    let invalid = pandar_plugin_sync_ams_filaments(false);
    assert_eq!(invalid.status, -1);
    assert_eq!(take_body(invalid), r#"{"error":"invalid_handle"}"#);

    let unsupported = pandar_plugin_sync_ams_filaments(true);
    assert_eq!(unsupported.status, -32);
    assert_eq!(
        take_body(unsupported),
        r#"{"error":"unsupported_ams_sync"}"#
    );
}

#[test]
fn slot_mappings_sync_returns_the_stable_explicit_unsupported_contract() {
    let invalid = pandar_plugin_sync_slot_mappings(false);
    assert_eq!(invalid.status, -1);
    assert_eq!(take_body(invalid), r#"{"error":"invalid_handle"}"#);

    let unsupported = pandar_plugin_sync_slot_mappings(true);
    assert_eq!(unsupported.status, -33);
    assert_eq!(
        take_body(unsupported),
        r#"{"error":"unsupported_slot_mappings_sync"}"#
    );
}

fn take_body(result: PluginHttpResult) -> String {
    if result.body_ptr.is_null() || result.body_len == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(result.body_ptr, result.body_len) };
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    unsafe {
        pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap)
    };
    body
}
