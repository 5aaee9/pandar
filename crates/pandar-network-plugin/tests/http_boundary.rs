mod support;

use pandar_network_plugin::{
    PluginHttpResult, pandar_plugin_exchange_ticket, pandar_plugin_free_with_capacity,
    pandar_plugin_get_jobs, pandar_plugin_get_printers, pandar_plugin_submit_print,
};
use std::{fs, io::Write, net::TcpListener, path::Path, thread};
use support::{
    assert_multipart_file_part, assert_multipart_print_request, read_http_request_with_timeout,
};

const TOKEN: &[u8] = b"pandar_plugin_test_token";

fn body(result: PluginHttpResult) -> String {
    if result.body_ptr.is_null() || result.body_len == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(result.body_ptr, result.body_len) };
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap);
    body
}

fn one_shot_server(
    expected_method: &'static str,
    expected_path: &'static str,
    expected_bearer: Option<&'static str>,
    status_line: &'static str,
    body: &'static str,
    inspect_request: Option<fn(&str)>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request_with_timeout(&mut stream, None);
        let mut lines = request.lines();
        assert_eq!(
            lines.next().unwrap(),
            format!("{expected_method} {expected_path} HTTP/1.1")
        );
        if let Some(token) = expected_bearer {
            assert!(
                request.contains(&format!("authorization: Bearer {token}")),
                "request did not contain expected bearer header: {request}"
            );
        }
        if let Some(inspect_request) = inspect_request {
            inspect_request(&request);
        }
        let response = format!(
            "{status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    url
}

fn assert_plugin_multipart_print_request(request: &str) {
    assert_multipart_print_request(request);
    assert_multipart_file_part(request, "job.3mf", b"not empty");
}

fn exchange_ticket(hub_url: &[u8], ticket: &[u8]) -> PluginHttpResult {
    pandar_plugin_exchange_ticket(
        hub_url.as_ptr(),
        hub_url.len(),
        ticket.as_ptr(),
        ticket.len(),
    )
}

fn get_printers(hub_url: &[u8], token: &[u8]) -> PluginHttpResult {
    pandar_plugin_get_printers(hub_url.as_ptr(), hub_url.len(), token.as_ptr(), token.len())
}

fn get_jobs(hub_url: &[u8], token: &[u8]) -> PluginHttpResult {
    pandar_plugin_get_jobs(hub_url.as_ptr(), hub_url.len(), token.as_ptr(), token.len())
}

fn submit_print(hub_url: &[u8], token: &[u8], artifact_path: &[u8]) -> PluginHttpResult {
    let printer_id = b"printer";
    let filename = b"job.3mf";
    pandar_plugin_submit_print(
        hub_url.as_ptr(),
        hub_url.len(),
        token.as_ptr(),
        token.len(),
        printer_id.as_ptr(),
        printer_id.len(),
        filename.as_ptr(),
        filename.len(),
        artifact_path.as_ptr(),
        artifact_path.len(),
        1,
        true,
        false,
        false,
        b"".as_ptr(),
        0,
        b"".as_ptr(),
        0,
    )
}

fn write_artifact(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
}

#[test]
fn invalid_hub_url_is_rejected_before_network() {
    let result = exchange_ticket(b"", b"ticket");

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"invalid_hub_url"}"#);
}

#[test]
fn syntactically_invalid_hub_url_is_rejected_before_network() {
    let result = exchange_ticket(b"not a hub url", b"ticket");

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"invalid_hub_url"}"#);
}

#[test]
fn network_failure_maps_to_hub_unavailable() {
    let result = exchange_ticket(b"http://127.0.0.1:9", b"ticket");

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 0);
    assert_eq!(body(result), r#"{"error":"hub_unavailable"}"#);
}

#[test]
fn ticket_exchange_401_maps_to_invalid_plugin_ticket() {
    let hub_url = one_shot_server(
        "POST",
        "/api/v1/plugin/login-tickets/exchange",
        None,
        "HTTP/1.1 401 Unauthorized",
        r#"{"error":"secret ticket"}"#,
        None,
    );
    let result = exchange_ticket(hub_url.as_bytes(), b"ticket");

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 401);
    assert_eq!(body(result), r#"{"error":"invalid_plugin_ticket"}"#);
}

#[test]
fn empty_auth_token_is_rejected_before_network() {
    let result = get_printers(b"http://127.0.0.1:9", b"   ");

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"invalid_auth_token"}"#);
}

#[test]
fn authenticated_401_maps_to_invalid_auth_token() {
    let hub_url = one_shot_server(
        "GET",
        "/api/v1/plugin/printers",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 401 Unauthorized",
        r#"{"error":"secret token"}"#,
        None,
    );
    let result = get_printers(hub_url.as_bytes(), TOKEN);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 401);
    assert_eq!(body(result), r#"{"error":"invalid_auth_token"}"#);
}

#[test]
fn forbidden_maps_to_plugin_forbidden() {
    let hub_url = one_shot_server(
        "GET",
        "/api/v1/plugin/printers",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 403 Forbidden",
        r#"{"error":"tenant xyz"}"#,
        None,
    );
    let result = get_printers(hub_url.as_bytes(), TOKEN);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 403);
    assert_eq!(body(result), r#"{"error":"plugin_forbidden"}"#);
}

#[test]
fn not_found_without_stable_code_maps_to_printer_not_found() {
    let hub_url = one_shot_server(
        "GET",
        "/api/v1/plugin/printers",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 404 Not Found",
        r#"{"error":"missing /tmp/x"}"#,
        None,
    );
    let result = get_printers(hub_url.as_bytes(), TOKEN);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 404);
    assert_eq!(body(result), r#"{"error":"printer_not_found"}"#);
}

#[test]
fn jobs_not_found_without_stable_code_maps_to_printer_not_found() {
    let hub_url = one_shot_server(
        "GET",
        "/api/v1/plugin/jobs",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 404 Not Found",
        r#"{"error":"missing /tmp/job"}"#,
        None,
    );
    let result = get_jobs(hub_url.as_bytes(), TOKEN);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 404);
    assert_eq!(body(result), r#"{"error":"printer_not_found"}"#);
}

#[test]
fn print_not_found_without_stable_code_maps_to_printer_not_found() {
    let artifact =
        std::env::temp_dir().join(format!("pandar-print-not-found-{}.3mf", std::process::id()));
    write_artifact(&artifact, b"not empty");
    let artifact_path = artifact.to_string_lossy();
    let hub_url = one_shot_server(
        "POST",
        "/api/v1/plugin/prints",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 404 Not Found",
        r#"{"error":"missing /tmp/print"}"#,
        Some(assert_plugin_multipart_print_request),
    );
    let result = submit_print(hub_url.as_bytes(), TOKEN, artifact_path.as_bytes());
    fs::remove_file(&artifact).unwrap();

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 404);
    assert_eq!(body(result), r#"{"error":"printer_not_found"}"#);
}

#[test]
fn token_revoked_body_maps_to_plugin_token_revoked() {
    let hub_url = one_shot_server(
        "GET",
        "/api/v1/plugin/printers",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 400 Bad Request",
        r#"{"error":"token_revoked"}"#,
        None,
    );
    let result = get_printers(hub_url.as_bytes(), TOKEN);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"plugin_token_revoked"}"#);
}

#[test]
fn unrecognized_server_error_maps_to_invalid_response() {
    let hub_url = one_shot_server(
        "GET",
        "/api/v1/plugin/printers",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 500 Internal Server Error",
        r#"{"error":"db password"}"#,
        None,
    );
    let result = get_printers(hub_url.as_bytes(), TOKEN);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 500);
    assert_eq!(body(result), r#"{"error":"invalid_response"}"#);
}

#[test]
fn empty_artifact_is_rejected_before_network() {
    let artifact =
        std::env::temp_dir().join(format!("pandar-empty-artifact-{}.3mf", std::process::id()));
    write_artifact(&artifact, b"");
    let artifact_path = artifact.to_string_lossy();
    let result = submit_print(b"http://127.0.0.1:9", TOKEN, artifact_path.as_bytes());
    fs::remove_file(&artifact).unwrap();

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"artifact_empty"}"#);
}

#[test]
fn missing_artifact_is_rejected_without_leaking_path() {
    let artifact_path = b"/tmp/pandar-secret-path/job.3mf";
    let result = submit_print(b"http://127.0.0.1:9", TOKEN, artifact_path);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    let response_body = body(result);
    assert_eq!(response_body, r#"{"error":"artifact_missing"}"#);
    assert!(!response_body.contains("pandar-secret-path"));
}

#[test]
fn hub_artifact_errors_pass_through_when_stable() {
    let artifact = std::env::temp_dir().join(format!("pandar-artifact-{}.3mf", std::process::id()));
    write_artifact(&artifact, b"not empty");
    let artifact_path = artifact.to_string_lossy();
    let hub_url = one_shot_server(
        "POST",
        "/api/v1/plugin/prints",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 400 Bad Request",
        r#"{"error":"artifact_invalid_upload"}"#,
        Some(assert_plugin_multipart_print_request),
    );
    let result = submit_print(hub_url.as_bytes(), TOKEN, artifact_path.as_bytes());
    fs::remove_file(&artifact).unwrap();

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"artifact_invalid_upload"}"#);
}

#[test]
fn retired_base64_artifact_error_is_not_stable() {
    let artifact = std::env::temp_dir().join(format!(
        "pandar-retired-base64-error-{}.3mf",
        std::process::id()
    ));
    write_artifact(&artifact, b"not empty");
    let artifact_path = artifact.to_string_lossy();
    let hub_url = one_shot_server(
        "POST",
        "/api/v1/plugin/prints",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 400 Bad Request",
        Box::leak(
            format!(
                r#"{{"error":"{}"}}"#,
                ["artifact", "invalid", "base64"].join("_")
            )
            .into_boxed_str(),
        ),
        Some(assert_plugin_multipart_print_request),
    );
    let result = submit_print(hub_url.as_bytes(), TOKEN, artifact_path.as_bytes());
    fs::remove_file(&artifact).unwrap();

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"invalid_response"}"#);
}

#[test]
fn exchange_ticket_rejects_empty_ticket_before_network() {
    let hub = b"http://127.0.0.1:9";
    let result = pandar_plugin_exchange_ticket(hub.as_ptr(), hub.len(), b"".as_ptr(), 0);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"invalid_plugin_ticket"}"#);
}
