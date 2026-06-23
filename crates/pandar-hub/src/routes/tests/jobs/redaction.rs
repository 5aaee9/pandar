#[test]
fn artifact_error_redaction_removes_print_storage_paths() {
    let message = "failed to remove artifact file /tmp/pandar/spool/tenant/artifact/plate.3mf\n\nCaused by:\n    permission denied";

    let redacted = crate::routes::jobs::redact_artifact_error(message);

    assert!(redacted.contains("failed to remove artifact file [redacted]"));
    assert!(redacted.contains("Caused by:"));
    assert!(redacted.contains("permission denied"));
    assert!(!redacted.contains("/tmp/pandar"));
    assert!(!redacted.contains("plate.3mf"));
}
