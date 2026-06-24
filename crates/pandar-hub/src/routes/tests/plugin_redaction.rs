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

#[test]
fn artifact_error_redaction_covers_agent_download_open_failures() {
    let message = "failed to read artifact from storage\n\nCaused by:\n    failed to open artifact file /spool/tenant/artifact/secret.3mf\n    permission denied";

    let redacted = crate::routes::plugin::redact_artifact_error(message);

    assert!(redacted.contains("failed to open artifact file [redacted]"));
    assert!(redacted.contains("permission denied"));
    assert!(!redacted.contains("/spool"));
    assert!(!redacted.contains("secret.3mf"));
}
