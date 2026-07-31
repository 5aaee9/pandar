use std::ffi::CStr;

use pandar_network_plugin::pandar_plugin_network_agent_version;

#[path = "studio_target_abi/lifecycle_boundary.rs"]
mod lifecycle_boundary;
#[path = "studio_target_abi/source_hygiene.rs"]
mod source_hygiene;

#[test]
fn network_agent_version_matches_pinned_studio_target() {
    let version = unsafe { CStr::from_ptr(pandar_plugin_network_agent_version()) }
        .to_str()
        .unwrap();

    assert_eq!(version, "02.07.01.51");
    assert_eq!(&version[..8], "02.07.01");
}

#[test]
fn cache_admission_failure_preserves_pending_connection_deliveries() {
    let lifecycle = include_str!("../src/connection/no_auth_refresh.rs");
    let finalize = lifecycle
        .split_once("unsafe extern \"C\" fn finalize_refresh")
        .expect("Rust printer-refresh finalization")
        .1
        .split_once("fn with_refresh_lock")
        .expect("Rust printer-refresh finalization end")
        .0;
    assert!(
        finalize.contains("if failed || finalization.snapshot_current")
            && finalize.contains("take_transition()")
            && finalize.contains("take_offline()"),
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
    let state_entry = state
        .split_once("bool refresh_printer_status_cache")
        .expect("background refresh entry")
        .1
        .split_once("} // namespace pandar::network_plugin")
        .expect("background refresh entry end")
        .0;
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

    let status = include_str!("../src/shim_status_delivery.hpp");
    assert!(
        status.contains("pandar_plugin_studio_claim_delivery(")
            && status.contains("pandar_plugin_studio_complete_delivery("),
        "C++ callback adapter bypasses Rust's two-phase final claim"
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
    let server = connection
        .split_once("void dispatch_connection_transition")
        .expect("server transition dispatcher")
        .1
        .split_once("bool connection_printer_eligible_under_refresh")
        .expect("server transition dispatcher end")
        .0;
    assert!(
        server
            .matches("std::lock_guard<std::recursive_timed_mutex> gate(agent->callback_mutex)")
            .count()
            >= 2,
        "server reachability and auth callbacks can overtake claimed state"
    );

    let offline = connection
        .split_once("void dispatch_issued_printer_offline_transitions")
        .expect("offline transition dispatcher")
        .1;
    assert!(
        connection.contains("pandar_plugin_studio_take_work(")
            && offline.contains("pandar_plugin_studio_claim_delivery(")
            && offline.contains("pandar_plugin_studio_complete_delivery(")
            && offline.contains("gate(agent->callback_mutex)")
            && offline.contains("callback = agent->on_local_connect")
            && offline.contains("delivery.kind == 3"),
        "offline message and Lost callbacks bypass the Rust-owned two-phase final claim"
    );

    let connected = include_str!("../src/shim_status.hpp")
        .split_once("bool emit_cloud_printer_connected_signal")
        .expect("cloud connected signal")
        .1
        .split_once("bool emit_printer_status")
        .expect("cloud connected signal end")
        .0;
    assert!(
        connected.contains("gate(agent->callback_mutex)"),
        "cloud Connected can overtake a disconnected delivery"
    );

    let local_connected = include_str!("../src/shim_status_delivery.hpp")
        .split_once("bool emit_local_connect")
        .expect("local connected signal")
        .1;
    assert!(
        local_connected.contains("gate(agent->callback_mutex)"),
        "local Connected can overtake a Lost delivery"
    );

    let account_lost = include_str!("../src/shim_state.hpp")
        .split_once("finish_account_printer_transition")
        .expect("account Lost final claim")
        .1
        .split_once("LocalLostDelivery clear_login_state")
        .expect("account Lost final claim end")
        .0;
    assert!(
        account_lost.contains("dispatch_issued_printer_offline_transitions")
            && account_lost.contains("take_studio_offline_transitions")
            && account_lost.contains("pandar_plugin_studio_finish_account_transition"),
        "account Lost work bypasses the Rust-owned offline dispatcher"
    );
    assert!(
        account_lost.find("dispatch_issued_printer_offline_transitions")
            < account_lost.find("pandar_plugin_studio_finish_account_transition"),
        "account transition admission reopened before Lost delivery completed"
    );
}

#[test]
fn cpp_shim_contains_no_status_json_or_camera_policy() {
    let status_payload = include_str!("../src/shim_status_payload.hpp");
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
            !status_payload.contains(forbidden),
            "C++ status payload still owns typed JSON field {forbidden}"
        );
    }

    let camera_sources = [status_payload, content].join("\n");
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

#[test]
fn model_subtask_abi_uses_a_typed_worker_and_studio_owned_target() {
    let content = include_str!("../src/shim_abi_content.hpp");
    let model_task = include_str!("../src/shim_model_task.hpp");
    let model_types = include_str!("../src/shim_model_task_types.hpp");
    let print_types = include_str!("../src/shim_print_types.hpp");
    let user = include_str!("../src/shim_abi_user.hpp");
    let ffi = include_str!("../src/studio_print/model_task.rs");
    let body = content
        .split_once("PANDAR_ABI int bambu_network_get_subtask(")
        .expect("model subtask ABI")
        .1
        .split_once("PANDAR_ABI int bambu_network_get_model_mall_home_url")
        .expect("model subtask ABI end")
        .0;
    assert!(body.contains("enqueue_model_task(current, task, std::move(callback))"));
    assert!(body.contains("BAMBU_NETWORK_SUCCESS"));
    assert!(body.contains("BAMBU_NETWORK_ERR_INVALID_RESULT"));
    assert!(!body.contains("callback("));
    assert!(
        ffi.contains("pub struct PluginStudioModelTask")
            && ffi.contains("#[serde(deny_unknown_fields)]")
            && ffi.contains("pandar_plugin_studio_get_model_task")
            && print_types.contains("struct PluginStudioModelTask")
            && print_types.contains("pandar_plugin_studio_get_model_task_with_session")
    );
    assert!(
        model_types.contains("class BBLModelTask")
            && model_types.contains("std::string profile_name")
            && model_task.contains("start_model_task_worker")
            && model_task.contains("stop_model_task_worker")
            && model_task.contains("callback(target)")
            && user.contains("start_model_task_worker(agent)")
            && user.contains("stop_model_task_worker(a)")
    );
    assert!(
        model_task.contains("callback_gate(agent->callback_mutex)")
            && model_task.contains("account_gate(agent->account_mutex)")
            && model_task.contains("if (model_task_worker_stopping(agent)) return")
    );
}
