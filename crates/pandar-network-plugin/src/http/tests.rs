use serde::Serialize;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tempfile::NamedTempFile;

use super::{
    EmptyRequest, PrintSubmissionBody, post_json_with_connect_failure_with_writer,
    post_json_with_writer, post_multipart_print_with_writer, redact_hub_error,
};
use crate::{
    PluginHttpResult, RequestKind, pandar_plugin_create_no_auth_session,
    pandar_plugin_free_with_capacity, pandar_plugin_no_auth_retryable_connect_failure,
};
use pandar_core::PrintCalibrationMode;

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
    unsafe {
        pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap)
    };
    body
}

#[test]
fn printer_operation_network_failure_logs_complete_redacted_chain() {
    let url = "http://127.0.0.1:0/private/filesystem/secret-path";
    let mut diagnostic = Vec::new();

    let result = post_json_with_writer(
        url,
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
            || lower.contains("can't assign requested address")
            || lower.contains("os error 10061")
            || lower.contains("os error 10049"),
        "diagnostic lacked a refusal cause: {diagnostic}"
    );
    for secret in [
        url,
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

#[test]
fn response_body_network_failure_logs_complete_redacted_chain() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..read]);
        assert!(request.starts_with("POST /private/response-secret HTTP/1.1"));
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 64\r\nConnection: close\r\n\r\nshort")
            .unwrap();
    });
    let url = format!("http://{address}/private/response-secret");
    let mut diagnostic = Vec::new();

    let result = post_json_with_writer(
        &url,
        Some("response-secret-token"),
        SecretOperation {
            action: "response-secret-action",
            access_code: "response-secret-access-code",
        },
        RequestKind::PrinterOperation,
        &mut diagnostic,
    );
    server.join().unwrap();

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 200);
    assert_eq!(body(result), r#"{"error":"invalid_response"}"#);

    let diagnostic = String::from_utf8(diagnostic).unwrap();
    assert_eq!(diagnostic.lines().count(), 1, "diagnostic: {diagnostic}");
    assert!(diagnostic.contains("read plugin HTTP response body"));
    let lower = diagnostic.to_ascii_lowercase();
    assert!(
        lower.contains("body")
            && (lower.contains("end of file")
                || lower.contains("unexpected eof")
                || lower.contains("incomplete")
                || lower.contains("connection closed")),
        "diagnostic lacked the response-read root cause: {diagnostic}"
    );
    for secret in [
        url.as_str(),
        "Bearer",
        "response-secret-token",
        "response-secret-action",
        "response-secret-access-code",
        "response-secret",
    ] {
        assert!(
            !diagnostic.contains(secret),
            "diagnostic leaked {secret}: {diagnostic}"
        );
    }
}

#[test]
fn multipart_network_failure_logs_complete_redacted_chain() {
    let url = "http://127.0.0.1:0/private/multipart-secret";
    let mut artifact = NamedTempFile::new().unwrap();
    artifact.write_all(b"3mf-secret-content").unwrap();
    artifact.flush().unwrap();
    let artifact_path = artifact.path().to_path_buf();
    let artifact_len = artifact.as_file().metadata().unwrap().len();
    let mut diagnostic = Vec::new();

    let result = post_multipart_print_with_writer(
        url,
        "multipart-secret-token",
        PrintSubmissionBody {
            printer_id: "multipart-secret-printer".to_owned(),
            filename: "multipart-secret.3mf".to_owned(),
            artifact_path: artifact_path.clone(),
            artifact_len,
            plate_id: 1,
            use_ams: false,
            bed_leveling: false,
            auto_bed_leveling: PrintCalibrationMode::Off,
            flow_cali: false,
            auto_flow_cali: PrintCalibrationMode::Off,
            auto_offset_cali: PrintCalibrationMode::Off,
            timelapse: false,
            ams_mapping: None,
            ams_mapping2: None,
            ams_mapping_info: None,
        },
        &mut diagnostic,
    );

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 0);
    assert_eq!(body(result), r#"{"error":"hub_unavailable"}"#);

    let diagnostic = String::from_utf8(diagnostic).unwrap();
    assert_eq!(diagnostic.lines().count(), 1, "diagnostic: {diagnostic}");
    assert!(diagnostic.contains("POST plugin multipart print submission request"));
    let lower = diagnostic.to_ascii_lowercase();
    assert!(
        lower.contains("connection refused")
            || lower.contains("actively refused")
            || lower.contains("can't assign requested address")
            || lower.contains("os error 10061")
            || lower.contains("os error 10049"),
        "diagnostic lacked the multipart root cause: {diagnostic}"
    );
    for secret in [
        url,
        "Bearer",
        "multipart-secret-token",
        "multipart-secret-printer",
        "multipart-secret.3mf",
        artifact_path.to_string_lossy().as_ref(),
        "multipart-secret",
        "3mf-secret-content",
    ] {
        assert!(
            !diagnostic.contains(secret),
            "diagnostic leaked {secret}: {diagnostic}"
        );
    }
}

#[test]
fn no_auth_retry_is_limited_to_connection_failures_before_request_delivery() {
    let url = "http://127.0.0.1:0";

    let connect_failure = unsafe { pandar_plugin_create_no_auth_session(url.as_ptr(), url.len()) };

    assert!(pandar_plugin_no_auth_retryable_connect_failure(
        connect_failure.status
    ));
    assert_eq!(connect_failure.http_code, 0);
    assert_eq!(body(connect_failure), r#"{"error":"hub_unavailable"}"#);

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..read]);
        assert!(request.starts_with("POST /api/v1/plugin/no-auth-session HTTP/1.1"));
    });
    let url = format!("http://{address}");

    let response_lost = unsafe { pandar_plugin_create_no_auth_session(url.as_ptr(), url.len()) };
    server.join().unwrap();

    assert!(!pandar_plugin_no_auth_retryable_connect_failure(
        response_lost.status
    ));
    assert_eq!(response_lost.http_code, 0);
    assert_eq!(body(response_lost), r#"{"error":"hub_unavailable"}"#);
}

#[test]
fn no_auth_accepted_silent_hub_returns_without_connect_failure_retry() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..read]);
        assert!(request.starts_with("POST /api/v1/plugin/no-auth-session HTTP/1.1"));
        let _ = release_rx.recv_timeout(Duration::from_secs(7));
    });
    let url = format!("http://{address}/api/v1/plugin/no-auth-session");
    let mut diagnostic = Vec::new();

    let started = Instant::now();
    let result = post_json_with_connect_failure_with_writer(
        &url,
        EmptyRequest {},
        RequestKind::TicketExchange,
        &mut diagnostic,
    );
    let elapsed = started.elapsed();
    let _ = release_tx.send(());
    server.join().unwrap();

    assert!(
        elapsed < Duration::from_secs(6),
        "accepted silent Hub blocked no-auth session creation for {elapsed:?}"
    );
    assert!(!pandar_plugin_no_auth_retryable_connect_failure(
        result.status
    ));
    assert_eq!(result.http_code, 0);
    assert_eq!(body(result), r#"{"error":"hub_unavailable"}"#);

    let diagnostic = String::from_utf8(diagnostic).unwrap();
    assert_eq!(diagnostic.lines().count(), 1, "diagnostic: {diagnostic}");
    assert!(diagnostic.contains("POST plugin authentication request"));
    assert!(
        diagnostic.to_ascii_lowercase().contains("timed out"),
        "diagnostic lacked the timeout root cause: {diagnostic}"
    );
    assert!(
        !diagnostic.contains(&url),
        "diagnostic leaked URL: {diagnostic}"
    );
    assert!(
        !diagnostic.contains("no-auth-session"),
        "diagnostic leaked URL path: {diagnostic}"
    );
}

#[test]
fn no_auth_silent_response_body_returns_a_redacted_read_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..read]);
        assert!(request.starts_with("POST /api/v1/plugin/no-auth-session HTTP/1.1"));
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 32\r\nConnection: close\r\n\r\n")
            .unwrap();
        thread::sleep(Duration::from_secs(7));
        drop(stream);
    });
    let url = format!("http://{address}/api/v1/plugin/no-auth-session");
    let mut diagnostic = Vec::new();

    let started = Instant::now();
    let result = post_json_with_connect_failure_with_writer(
        &url,
        EmptyRequest {},
        RequestKind::TicketExchange,
        &mut diagnostic,
    );
    let elapsed = started.elapsed();
    server.join().unwrap();

    assert!(
        elapsed < Duration::from_secs(6),
        "silent no-auth response body blocked for {elapsed:?}"
    );
    assert!(!pandar_plugin_no_auth_retryable_connect_failure(
        result.status
    ));
    assert_eq!(result.http_code, 200);
    assert_eq!(body(result), r#"{"error":"invalid_response"}"#);

    let diagnostic = String::from_utf8(diagnostic).unwrap();
    assert_eq!(diagnostic.lines().count(), 1, "diagnostic: {diagnostic}");
    assert!(diagnostic.contains("read plugin HTTP response body"));
    assert!(
        diagnostic.to_ascii_lowercase().contains("timed out"),
        "diagnostic lacked the response timeout root cause: {diagnostic}"
    );
    assert!(
        !diagnostic.contains(&url),
        "diagnostic leaked URL: {diagnostic}"
    );
    assert!(
        !diagnostic.contains("no-auth-session"),
        "diagnostic leaked URL path: {diagnostic}"
    );
}

#[test]
fn generic_http_rejects_declared_oversized_response_without_waiting_for_body() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1048577\r\nConnection: close\r\n\r\n")
            .unwrap();
        thread::sleep(Duration::from_secs(1));
    });
    let url = format!("http://{address}/api/v1/plugin/no-auth-session");
    let mut diagnostic = Vec::new();

    let started = Instant::now();
    let result = post_json_with_connect_failure_with_writer(
        &url,
        EmptyRequest {},
        RequestKind::TicketExchange,
        &mut diagnostic,
    );
    let elapsed = started.elapsed();
    server.join().unwrap();

    assert!(elapsed < Duration::from_millis(500));
    assert_eq!(result.http_code, 200);
    assert_eq!(body(result), r#"{"error":"invalid_response"}"#);
    assert!(
        String::from_utf8(diagnostic)
            .unwrap()
            .contains("exceeds 1048576 bytes")
    );
}

#[test]
fn no_auth_tenant_selection_errors_remain_stable() {
    assert_eq!(
        redact_hub_error(
            RequestKind::TicketExchange,
            409,
            r#"{"error":"ambiguous_no_auth_tenant"}"#,
        ),
        r#"{"error":"ambiguous_no_auth_tenant"}"#
    );
    assert_eq!(
        redact_hub_error(
            RequestKind::TicketExchange,
            404,
            r#"{"error":"tenant_not_found"}"#,
        ),
        r#"{"error":"tenant_not_found"}"#
    );
}
