#[test]
fn printer_transport_credentials_do_not_cross_the_plugin_abi() {
    let rust_sources = [
        include_str!("../src/studio_status/list.rs"),
        include_str!("../src/connection.rs"),
        include_str!("../src/connection/ffi.rs"),
        include_str!("../src/studio_abi.rs"),
    ]
    .join("\n");
    for forbidden in [
        "dev_access_code",
        "access_code",
        "host_ptr",
        "host_len",
        "pub(crate) host",
    ] {
        assert!(
            !rust_sources.contains(forbidden),
            "printer transport secret crossed the Rust plugin ABI: {forbidden}"
        );
    }

    let cxx_sources = [
        include_str!("../src/shim_types.hpp"),
        include_str!("../src/shim_connection.hpp"),
        include_str!("../src/shim_state.hpp"),
        include_str!("../src/shim_printer_cache.hpp"),
        include_str!("../src/shim_status_payload.hpp"),
    ]
    .join("\n");
    for forbidden in ["printer_connections", "host_ptr", "access_ptr"] {
        assert!(
            !cxx_sources.contains(forbidden),
            "printer transport secret crossed the compiled C++ cache ABI: {forbidden}"
        );
    }
}

#[test]
fn unknown_printer_identity_is_not_replaced_with_a_different_model_or_state() {
    for source in [
        include_str!("../src/studio_status/list.rs"),
        include_str!("../src/studio_status/device.rs"),
        include_str!("../src/shim_printer_cache.hpp"),
    ] {
        assert!(
            !source.contains("C11"),
            "unknown model was replaced with C11"
        );
        assert!(
            !source.contains("\"IDLE\""),
            "unknown state was replaced with IDLE"
        );
    }
}

#[test]
fn rust_owns_file_transfer_errors_and_firmware_identity_admission() {
    let transfer = include_str!("../src/file_transfer.rs");
    let firmware = include_str!("../src/shim_firmware.hpp");
    let firmware_request = include_str!("../src/shim_firmware_request.hpp");
    let policy = include_str!("../src/studio_policy.rs");
    let firmware_ffi = include_str!("../src/firmware/ffi.rs");

    assert!(!transfer.contains(r#"R"({"#));
    assert!(
        transfer.contains(r#"stable_error_body("unsupported_file_transfer")"#)
            && policy.contains("pandar_plugin_studio_file_transfer_unavailable")
    );
    assert!(!firmware.contains("normalized_dev_id.empty() || printer_id.empty()"));
    assert!(
        firmware.contains("pandar_plugin_studio_request_admitted(")
            && firmware.contains("firmware_send_from_snapshot(")
            && firmware_request.contains("snapshot.firmware_generation")
            && firmware_ffi
                .contains("studio_dev_id.trim().is_empty() || printer_id.trim().is_empty()")
            && firmware_ffi.contains("invalid_input(\"invalid_firmware_request\")")
    );
}
