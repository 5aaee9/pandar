#[test]
fn artifact_error_redaction_preserves_context_without_paths() {
    let message = "failed to write artifact file /tmp/pandar/spool/tenant/artifact/plate.3mf\n\nCaused by:\n    permission denied";

    let redacted = crate::routes::plugin::redact_artifact_error(message);

    assert!(redacted.contains("failed to write artifact file [redacted]"));
    assert!(redacted.contains("Caused by:"));
    assert!(redacted.contains("permission denied"));
    assert!(!redacted.contains("/tmp/pandar"));
    assert!(!redacted.contains("plate.3mf"));
}
