use serde::Serialize;
use std::net::TcpListener;

use super::post_json_with_writer;
use crate::{PluginHttpResult, RequestKind, pandar_plugin_free_with_capacity};

#[derive(Serialize)]
struct SecretOperation<'a> {
    action: &'a str,
    access_code: &'a str,
}

fn body(result: PluginHttpResult) -> String {
    if result.body_ptr.is_null() || result.body_len == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(result.body_ptr, result.body_len) };
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap);
    body
}

#[test]
fn printer_operation_network_failure_logs_complete_redacted_chain() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let url = format!("http://{address}/private/filesystem/secret-path");
    let mut diagnostic = Vec::new();

    let result = post_json_with_writer(
        &url,
        Some("secret-token"),
        SecretOperation {
            action: "secret-request-action",
            access_code: "secret-access-code",
        },
        RequestKind::PrinterOperation,
        &mut diagnostic,
    );

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 0);
    assert_eq!(body(result), r#"{"error":"hub_unavailable"}"#);

    let diagnostic = String::from_utf8(diagnostic).unwrap();
    assert_eq!(diagnostic.lines().count(), 1, "diagnostic: {diagnostic}");
    assert!(diagnostic.starts_with("pandar network plugin request failed: "));
    assert!(diagnostic.contains("POST plugin printer operation request"));
    let lower = diagnostic.to_ascii_lowercase();
    assert!(
        lower.contains("connection refused")
            || lower.contains("actively refused")
            || lower.contains("os error 10061"),
        "diagnostic lacked a refusal cause: {diagnostic}"
    );
    for secret in [
        url.as_str(),
        "Bearer",
        "secret-token",
        "secret-request-action",
        "secret-access-code",
        "secret-path",
        "filesystem",
    ] {
        assert!(
            !diagnostic.contains(secret),
            "diagnostic leaked {secret}: {diagnostic}"
        );
    }
}
