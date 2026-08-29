#[test]
fn cpp_no_auth_adapters_cannot_orchestrate_session_lifecycle() {
    let no_auth = include_str!("../../src/shim_no_auth.hpp");
    let tasks = include_str!("../../src/shim_tasks.hpp");
    let firmware = include_str!("../../src/shim_firmware.hpp");
    let account = include_str!("../../src/shim_abi_account.hpp");
    let account_ffi = include_str!("../../src/shim_account_ffi.hpp");
    let source_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut shim_paths = std::fs::read_dir(source_dir)
        .expect("plugin source directory")
        .map(|entry| entry.expect("plugin source entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name == "shim.cpp" || (name.starts_with("shim_") && name.ends_with(".hpp"))
                })
        })
        .collect::<Vec<_>>();
    shim_paths.sort();
    let cpp_account_boundary = shim_paths
        .iter()
        .map(|path| std::fs::read_to_string(path).expect("C++ shim source"))
        .collect::<Vec<_>>()
        .join("\n");

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
    ] {
        assert!(
            !cpp_account_boundary.contains(forbidden),
            "C++ shim still exposes low-level no-auth lifecycle operation {forbidden}"
        );
    }

    assert_eq!(
        no_auth
            .matches("pandar_plugin_account_no_auth_bootstrap(")
            .count(),
        1,
        "no-auth adapter must invoke exactly one Rust bootstrap lifecycle"
    );
    assert_eq!(
        account_ffi
            .matches("pandar_plugin_account_no_auth_bootstrap(")
            .count(),
        1,
        "C++ must expose one flat Rust bootstrap lifecycle declaration"
    );
    assert_eq!(
        account.matches("pandar_plugin_account_logout(").count(),
        1,
        "logout adapter must invoke exactly one Rust lifecycle"
    );
    assert_eq!(
        account_ffi.matches("pandar_plugin_account_logout(").count(),
        1,
        "C++ must expose one flat Rust logout lifecycle declaration"
    );
    for lifecycle in [
        "pandar_plugin_studio_get_tasks_with_session(",
        "pandar_plugin_studio_get_plate_with_session(",
        "pandar_plugin_studio_get_subtask_with_session(",
    ] {
        assert_eq!(
            tasks.matches(lifecycle).count(),
            1,
            "task adapter must invoke exactly one Rust lifecycle {lifecycle}"
        );
    }
    assert!(
        !firmware.contains("get_printers_with_token_refresh"),
        "firmware shim must not own printer-refresh lifecycle orchestration"
    );

    let transaction = include_str!("../../src/shim_account_transaction.hpp");
    assert!(
        transaction.contains("with_current_account")
            && transaction.contains("PluginAccountView")
            && transaction.contains("PluginAccountMutation"),
        "C++ must retain the pure account transaction ABI adapter"
    );

    let logout_lifecycle = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/account/lifecycle/logout.rs"),
    )
    .expect("Rust-owned logout lifecycle");
    let rust_lifecycles = [
        include_str!("../../src/account/lifecycle.rs"),
        include_str!("../../src/studio_print/session_recovery.rs"),
        include_str!("../../src/connection/no_auth_refresh.rs"),
        logout_lifecycle.as_str(),
    ]
    .join("\n");
    for lifecycle in [
        "pub unsafe extern \"C\" fn pandar_plugin_account_no_auth_bootstrap(",
        "pub unsafe extern \"C\" fn pandar_plugin_studio_get_tasks_with_session(",
        "pub unsafe extern \"C\" fn pandar_plugin_studio_get_plate_with_session(",
        "pub unsafe extern \"C\" fn pandar_plugin_studio_get_subtask_with_session(",
        "pub extern \"C\" fn pandar_plugin_printer_refresh_with_session(",
        "extern \"C\" fn pandar_plugin_account_logout(",
    ] {
        assert!(
            rust_lifecycles.contains(lifecycle),
            "Rust-owned lifecycle FFI is missing {lifecycle}"
        );
    }
}

#[test]
fn cpp_printer_refresh_entries_are_single_flat_lifecycle_adapters() {
    let content = include_str!("../../src/shim_abi_content.hpp");
    let studio = content
        .split_once("PANDAR_ABI int bambu_network_get_user_print_info")
        .expect("Studio print-info entry")
        .1
        .split_once("PANDAR_ABI int bambu_network_get_printer_firmware")
        .expect("Studio print-info entry end")
        .0;
    for (name, entry) in [("Studio print-info", studio)] {
        assert_eq!(
            entry
                .matches("pandar_plugin_printer_refresh_with_session(")
                .count(),
            1,
            "{name} adapter must invoke exactly one Rust printer-refresh lifecycle"
        );
        for forbidden in [
            "pandar_plugin_studio_print_info_admission",
            "pandar_plugin_studio_account_request_admitted",
            "printer_cache_admission_pending",
            "begin_printer_cache_admission",
            "finish_printer_cache_admission",
            "get_printers_with_token_refresh",
            "remember_printer_cache",
            "invalidate_printer_cache_observation",
            "observe_firmware_printers",
            "take_connection_transition",
            "take_printer_offline_transitions",
            "pandar_plugin_studio_print_info_result",
        ] {
            assert!(
                !entry.contains(forbidden),
                "{name} adapter still orchestrates printer refresh via {forbidden}"
            );
        }
    }
}

#[test]
fn account_clear_notification_is_a_typed_rust_decision() {
    let transaction = include_str!("../../src/shim_account_transaction.hpp");
    let ffi = include_str!("../../src/shim_account_ffi.hpp");
    let rust_transaction = include_str!("../../src/account/lifecycle/transaction.rs");
    let rust_session = include_str!("../../src/account/session/mutation.rs");
    let logout = include_str!("../../src/account/lifecycle/logout.rs");

    assert!(
        !transaction.contains("had_login"),
        "C++ account adapter still infers logout notification policy from local token state"
    );
    assert!(
        ffi.contains("notification") && rust_transaction.contains("PluginAccountNotification"),
        "account mutation ABI has no typed notification decision"
    );
    assert!(
        logout.contains("mutation.notification = PluginAccountNotification::Logout"),
        "Rust logout lifecycle does not request the typed logout notification"
    );
    assert!(
        rust_session.contains("mutation.notification == PluginAccountNotification::Logout"),
        "Rust account session does not execute the typed logout notification"
    );
}
