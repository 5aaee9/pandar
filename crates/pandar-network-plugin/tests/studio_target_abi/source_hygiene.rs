use std::path::Path;

#[test]
fn plugin_source_contains_no_shared_error_slot() {
    assert_source_tree(Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path());
}

fn assert_source_tree(path: &Path) {
    for entry in std::fs::read_dir(path).expect("network plugin source directory") {
        let entry = entry.expect("network plugin source entry");
        let path = entry.path();
        if path.is_dir() {
            assert_source_tree(&path);
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("UTF-8 plugin source");
        assert!(
            !source.contains("last_error"),
            "{} still contains the removed shared error slot",
            path.display()
        );
    }
}

#[test]
fn firmware_requests_cannot_bypass_snapshot_generation_claim() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let catalog = source(&root, "shim_abi_content.hpp");
    assert_call_block(
        &catalog,
        "PANDAR_ABI int bambu_network_get_printer_firmware",
        "PANDAR_ABI int bambu_network_get_camera_url",
        "firmware_catalog_from_snapshot",
        "pandar_plugin_firmware_catalog(",
    );

    let dispatch = source(&root, "dispatch/message.rs");
    let version = block(
        &dispatch,
        "fn emit_printer_version",
        "fn dispatch_firmware_message",
    );
    assert!(
        version.contains("refresh_version_json("),
        "firmware version refresh bypassed the session snapshot claim"
    );
    let send = block(
        &dispatch,
        "fn dispatch_firmware_message",
        "fn printer_operation_status",
    );
    assert!(
        send.contains("firmware_session.send("),
        "firmware send bypassed the session snapshot claim"
    );
    for fenced in [version, send] {
        assert!(
            !fenced.contains("(bridge.firmware_generation)(agent)"),
            "firmware request re-read the current generation instead of the fenced one"
        );
    }

    let helper = source(&root, "shim_firmware_request.hpp");
    assert_eq!(
        helper
            .matches("const PrinterRequestSnapshot& snapshot")
            .count(),
        1
    );
    assert_eq!(helper.matches("snapshot.firmware_generation").count(), 1);
    assert_eq!(helper.matches("snapshot.printer_id").count(), 2);
    assert!(!helper.contains("->firmware_generation"));
}

fn source(root: &Path, name: &str) -> String {
    std::fs::read_to_string(root.join(name)).expect("UTF-8 shim source")
}

fn block<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start = source.find(start).expect("source block start");
    let end = source[start..]
        .find(end)
        .map(|offset| start + offset)
        .expect("source block end");
    &source[start..end]
}

fn assert_call_block(source: &str, start: &str, end: &str, helper: &str, raw_call: &str) {
    let start = source.find(start).expect("firmware call block start");
    let end = source[start..]
        .find(end)
        .map(|offset| start + offset)
        .expect("firmware call block end");
    let block = &source[start..end];
    assert!(
        block.contains(helper),
        "firmware call block bypassed {helper}"
    );
    assert!(
        !block.contains(raw_call),
        "firmware call block used raw FFI"
    );
    assert!(
        !block.contains("->firmware_generation"),
        "firmware call block re-read the current generation"
    );
}
