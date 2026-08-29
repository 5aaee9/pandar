use std::ffi::CStr;

use pandar_network_plugin::{
    PluginHttpResult, STUDIO_ABI_SERIES, pandar_plugin_free_with_capacity,
    pandar_plugin_network_agent_version, pandar_plugin_sync_ams_filaments,
};

#[path = "studio_target_abi/lifecycle_boundary.rs"]
mod lifecycle_boundary;
#[path = "studio_target_abi/source_hygiene.rs"]
mod source_hygiene;

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

fn take_body(result: PluginHttpResult) -> String {
    if result.body_ptr.is_null() || result.body_len == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(result.body_ptr, result.body_len) };
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap);
    body
}

#[test]
fn cache_admission_failure_preserves_pending_connection_deliveries() {
    let lifecycle = include_str!("../src/connection/no_auth_refresh.rs");
    let finalize = lifecycle
        .split_once("unsafe extern \"C\" fn finalize_serve")
        .expect("Rust printer-refresh finalization")
        .1
        .split_once("fn with_refresh_lock")
        .expect("Rust printer-refresh finalization end")
        .0;
    assert!(
        finalize.contains("take_transition()") && finalize.contains("take_offline()"),
        "Rust finalization must preserve deliveries for a successful stale cache response"
    );
    let source = include_str!("../src/shim_abi_content.hpp");
    let state = include_str!("../src/shim_state.hpp");
    let content_entry = source
        .split_once("PANDAR_ABI int bambu_network_get_user_print_info")
        .expect("Studio print-info entry")
        .1
        .split_once("PANDAR_ABI int bambu_network_get_printer_firmware")
        .expect("Studio print-info entry end")
        .0;
    // The retired background poll adapter is gone entirely; the whole state
    // header must now be free of cache-freshness and delivery ownership.
    assert!(
        !state.contains("bool refresh_printer_status_cache"),
        "retired background printer-list refresh still exists in the shim"
    );
    let state_entry = state;
    for adapter in [content_entry, state_entry] {
        assert!(
            !adapter.contains("remember_printer_cache")
                && !adapter.contains("take_connection_transition"),
            "C++ printer-refresh adapter still owns cache freshness or delivery extraction"
        );
    }
    let selection_source = include_str!("../src/shim_abi_user.hpp");
    let selection = selection_source
        .split_once("std::string bambu_network_get_user_selected_machine")
        .expect("selected-machine getter")
        .1
        .split_once("std::string bambu_network_get_studio_info_url")
        .expect("selected-machine getter end")
        .0;
    assert!(
        selection.contains("pandar_plugin_studio_selected")
            && !selection.contains("refresh_printer_status_cache")
            && !selection.contains("add_subscribe"),
        "selected-machine getter is not a pure Rust-session lookup"
    );
}

#[test]
fn printer_refresh_cache_admission_and_delivery_share_serialization_boundary() {
    let types = include_str!("../src/shim_types.hpp");
    let state = include_str!("../src/shim_state.hpp");
    assert!(
        types.contains("std::recursive_mutex printer_refresh_mutex"),
        "Agent lacks the end-to-end printer refresh/cache serialization boundary"
    );
    for removed in [
        "std::uint64_t printer_cache_generation",
        "selected_machine",
        "cloud_subscribed_devices",
        "active_local_device",
    ] {
        assert!(
            !types.contains(removed),
            "C++ Agent still owns Rust Studio-session state: {removed}"
        );
    }

    let rust_session = [
        include_str!("../src/connection/studio.rs"),
        include_str!("../src/connection/studio/delivery.rs"),
        include_str!("../src/connection/studio/session.rs"),
    ]
    .join("\n");
    for required in [
        "cache_generation: self.cache_generation",
        "delivery.cache_generation != self.studio.cache_generation",
        "fn claim_delivery(&mut self, ticket: u64)",
        "fn complete_delivery(&mut self, ticket: u64, delivered: bool)",
    ] {
        assert!(
            rust_session.contains(required),
            "Rust Studio session is missing delivery fence {required}"
        );
    }

    let status = [
        include_str!("../src/dispatch.rs"),
        include_str!("../src/dispatch/message.rs"),
    ]
    .join("\n");
    assert!(
        status.contains("studio_claim_delivery(") && status.contains("studio_complete_delivery("),
        "Rust message dispatch bypasses the session's two-phase final claim"
    );

    let request_snapshot = include_str!("../src/shim_request_snapshot.hpp");
    assert!(
        request_snapshot.contains("printer_refresh_mutex")
            && request_snapshot.contains("pandar_plugin_studio_request_snapshot(")
            && request_snapshot.contains("pandar_plugin_connection_studio_snapshot_current("),
        "C++ must copy a synchronized Rust snapshot and delegate freshness policy back to Rust"
    );
    assert!(
        !request_snapshot.contains("agent->hub_url != snapshot.hub_url")
            && !request_snapshot.contains("agent->token != snapshot.token"),
        "C++ request snapshot helper must not implement freshness comparison policy"
    );
    let refresh_lifecycle = include_str!("../src/connection/no_auth_refresh.rs");
    assert!(
        state.contains("with_printer_refresh_lock")
            && state.contains("printer_refresh_mutex")
            && refresh_lifecycle.contains("begin_printer_cache_admission")
            && refresh_lifecycle.contains("printer_cache_snapshot_current")
            && refresh_lifecycle.contains("finish_printer_cache_admission")
            && !state.contains("pandar_plugin_studio_account_request_admitted")
            && !state.contains("pandar_plugin_studio_account_request_current"),
        "printer-refresh admission or freshness bypasses the Rust lifecycle"
    );
    let content = include_str!("../src/shim_abi_content.hpp");
    assert!(
        content.contains("printer_request_snapshot(a, normalized_dev_id)")
            && content.contains("printer_request_snapshot_current(a, snapshot)"),
        "firmware catalog response is not fenced by its printer cache observation"
    );
}

#[test]
fn agent_state_callbacks_share_final_claim_serialization() {
    let connection = include_str!("../src/shim_connection.hpp");
    let dispatch = include_str!("../src/connection/studio/shim_dispatch.rs");
    assert!(
        connection.contains("pandar_plugin_shim_dispatch_connection_transition(")
            && connection.contains("pandar_plugin_shim_dispatch_offline_deliveries(")
            && connection.contains("shim_gate_lock")
            && connection.contains("callback_mutex.lock()")
            && connection.contains("callback = agent->on_server_connected")
            && connection.contains("callback = agent->on_local_connect"),
        "shim must expose only gate/invoke trampolines to the Rust dispatch policy"
    );
    assert!(
        !connection.contains("pandar_plugin_studio_claim_delivery(")
            && !connection.contains("pandar_plugin_studio_complete_delivery(")
            && !connection.contains("pandar_plugin_connection_claim_delivery("),
        "delivery claim policy must not live in the C++ shim"
    );
    assert!(
        dispatch.contains("session.claim_delivery(result.transition_ticket)")
            && dispatch.contains("session.studio_claim_delivery(work.ticket)")
            && dispatch.contains("session.studio_complete_delivery(work.ticket")
            && dispatch.contains("CallbackGate::lock(bridge, agent)")
            && dispatch.contains("STUDIO_WORK_LOCAL_CONNECTED"),
        "Rust dispatch must own claim-before-invoke, kind selection, and completion"
    );

    let dispatch_message_source = [
        include_str!("../src/dispatch.rs"),
        include_str!("../src/dispatch/message.rs"),
    ]
    .join("\n");
    let connected = dispatch_message_source
        .split_once("fn emit_cloud_printer_connected")
        .expect("cloud connected signal")
        .1
        .split_once("fn emit_printer_status")
        .expect("cloud connected signal end")
        .0;
    assert!(
        connected.contains("CallbackGate::lock(bridge, agent)"),
        "cloud Connected can overtake a disconnected delivery"
    );

    let dispatch_pending_source = [
        include_str!("../src/dispatch.rs"),
        include_str!("../src/dispatch/pending.rs"),
    ]
    .join("\n");
    let local_connected = dispatch_pending_source
        .split_once("fn pandar_plugin_dispatch_connect_local")
        .expect("local connected signal")
        .1;
    assert!(
        local_connected.contains("CallbackGate::lock(bridge, agent)"),
        "local Connected can overtake a Lost delivery"
    );

    let account_transition = include_str!("../src/account/session/callbacks.rs")
        .split_once("AccountCallback::Transition(callback)")
        .expect("account Lost final claim")
        .1
        .split_once("impl ExpectedAccount")
        .expect("account Lost final claim end")
        .0;
    assert!(
        account_transition.contains("dispatch_transition_and_tickets")
            && account_transition.contains("pandar_plugin_studio_finish_account_transition"),
        "account Lost work bypasses the Rust-owned offline dispatcher"
    );
    assert!(
        account_transition.find("dispatch_transition_and_tickets")
            < account_transition.find("pandar_plugin_studio_finish_account_transition"),
        "account transition admission reopened before Lost delivery completed"
    );
}

#[test]
fn cpp_shim_contains_no_status_json_or_camera_policy() {
    let dispatch_adapter = include_str!("../src/shim_dispatch.hpp");
    let content = include_str!("../src/shim_abi_content.hpp");

    for forbidden in [
        r#"R"({"print"#,
        r#"\"wifi_signal\""#,
        r#"\"sdcard\""#,
        r#"\"ipcam_dev\""#,
        r#"\"liveview\""#,
        r#"\"rtsp_url\""#,
        r#"\"support_chamber\""#,
        r#"\"support_mqtt_alive\""#,
    ] {
        assert!(
            !dispatch_adapter.contains(forbidden),
            "C++ status path still owns typed JSON field {forbidden}"
        );
    }

    let camera_sources = [dispatch_adapter, content].join("\n");
    for forbidden in ["camera_url_for(", "bambu:///", "rtsps://", "rtsp://"] {
        assert!(
            !camera_sources.contains(forbidden),
            "C++ shim still owns camera URL policy {forbidden}"
        );
    }
}

#[test]
fn cpp_account_and_request_adapters_delegate_policy_to_rust() {
    let profile = include_str!("../src/shim_profile.hpp");
    let firmware = include_str!("../src/shim_firmware.hpp");
    let no_auth = include_str!("../src/shim_no_auth.hpp");
    let content = include_str!("../src/shim_abi_content.hpp");
    let operations = include_str!("../src/shim_abi_operations.hpp");
    let account = include_str!("../src/shim_abi_account.hpp");
    let user = include_str!("../src/shim_abi_user.hpp");
    let account_ffi = include_str!("../src/shim_account_ffi.hpp");
    let account_transaction = include_str!("../src/shim_account_transaction.hpp");
    let policy = include_str!("../src/studio_policy.rs");

    for source in [
        profile, firmware, no_auth, content, operations, account, user,
    ] {
        assert!(
            !source.contains(r#"R"({"#),
            "C++ shim still constructs a policy JSON response"
        );
    }
    for forbidden in [
        "agent->token.empty() || agent->profile_json.empty()",
        "agent->hub_url != expected_hub",
        "hub_url != agent->hub_url",
        "snapshot.token.empty() || snapshot.printer_id.empty()",
        "user_info.empty() || user_info == \"{}\"",
        "return !a->token.empty()",
        "*http_code = 501",
        "http_code = 501",
    ] {
        assert!(
            ![
                profile, firmware, no_auth, content, operations, account, user
            ]
            .join("\n")
            .contains(forbidden),
            "C++ shim still selects account or request policy: {forbidden}"
        );
    }
    for required in [
        "pandar_plugin_studio_request_admitted",
        "pandar_plugin_studio_firmware_catalog_result",
        "pandar_plugin_studio_printer_operation_result",
    ] {
        assert!(
            policy.contains(required),
            "Rust policy seam is missing {required}"
        );
    }
    let account_lifecycles = [
        include_str!("../src/account/lifecycle/authenticated.rs"),
        include_str!("../src/account/lifecycle/persisted.rs"),
        include_str!("../src/account/lifecycle/logout.rs"),
    ]
    .join("\n");
    for (lifecycle, adapter) in [
        ("pandar_plugin_account_change_user", account),
        ("pandar_plugin_account_exchange_ticket", account),
        ("pandar_plugin_account_profile", account),
        ("pandar_plugin_account_logout", account),
        ("pandar_plugin_account_refresh_runtime", firmware),
        ("pandar_plugin_account_load_persisted", user),
    ] {
        let call = format!("{lifecycle}(");
        assert_eq!(
            account_lifecycles.matches(&format!("fn {call}")).count(),
            1,
            "Rust account lifecycle is missing {lifecycle}"
        );
        assert_eq!(
            account_ffi.matches(&call).count(),
            1,
            "C++ must declare one flat account lifecycle {lifecycle}"
        );
        assert_eq!(
            adapter.matches(&call).count(),
            1,
            "C++ adapter must invoke one flat account lifecycle {lifecycle}"
        );
    }
    let types = include_str!("../src/shim_types.hpp");
    assert!(
        types.contains("std::recursive_mutex account_mutex")
            && account_transaction.contains("account(agent->account_mutex)")
            && account_transaction.contains("refresh(agent->printer_refresh_mutex)"),
        "flat account transactions are not serialized"
    );
}

#[path = "studio_target_abi/model_task.rs"]
mod model_task;
