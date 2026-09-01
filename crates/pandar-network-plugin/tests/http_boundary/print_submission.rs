use std::fs;

use super::{
    TOKEN, assert_plugin_multipart_print_request, body, one_shot_server, submit_print,
    submit_print_with_modes, write_artifact,
};

fn print_submission_with_response(
    status_line: &'static str,
    response_body: &'static str,
) -> pandar_network_plugin::PluginHttpResult {
    let artifact = std::env::temp_dir().join(format!(
        "pandar-print-submission-{}.3mf",
        std::process::id()
    ));
    write_artifact(&artifact, b"not empty");
    let artifact_path = artifact.to_string_lossy();
    let hub_url = one_shot_server(
        "POST",
        "/api/v1/plugin/prints",
        Some("pandar_plugin_test_token"),
        status_line,
        response_body,
        Some(assert_plugin_multipart_print_request),
    );
    let result = submit_print(hub_url.as_bytes(), TOKEN, artifact_path.as_bytes());
    fs::remove_file(&artifact).unwrap();
    result
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
fn invalid_calibration_modes_are_rejected_before_artifact_or_network_io() {
    for modes in [(3, 1, 0), (2, -1, 0), (2, 1, 3)] {
        let result = submit_print_with_modes(
            b"http://127.0.0.1:9",
            TOKEN,
            b"/missing/calibration-test.3mf",
            modes.0,
            modes.1,
            modes.2,
        );

        assert_ne!(result.status, 0);
        assert_eq!(result.http_code, 400);
        assert_eq!(body(result), r#"{"error":"bad_request"}"#);
    }
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
fn hub_invalid_printer_id_passes_through_when_stable() {
    let artifact = std::env::temp_dir().join(format!(
        "pandar-invalid-printer-id-{}.3mf",
        std::process::id()
    ));
    write_artifact(&artifact, b"not empty");
    let artifact_path = artifact.to_string_lossy();
    let hub_url = one_shot_server(
        "POST",
        "/api/v1/plugin/prints",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 400 Bad Request",
        r#"{"error":"invalid_printer_id"}"#,
        Some(assert_plugin_multipart_print_request),
    );
    let result = submit_print(hub_url.as_bytes(), TOKEN, artifact_path.as_bytes());
    fs::remove_file(&artifact).unwrap();

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"invalid_printer_id"}"#);
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
fn authenticated_401_maps_to_invalid_auth_token() {
    let result =
        print_submission_with_response("HTTP/1.1 401 Unauthorized", r#"{"error":"secret token"}"#);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 401);
    assert_eq!(body(result), r#"{"error":"invalid_auth_token"}"#);
}

#[test]
fn forbidden_maps_to_plugin_forbidden() {
    let result =
        print_submission_with_response("HTTP/1.1 403 Forbidden", r#"{"error":"tenant xyz"}"#);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 403);
    assert_eq!(body(result), r#"{"error":"plugin_forbidden"}"#);
}

#[test]
fn token_revoked_body_maps_to_plugin_token_revoked() {
    let result =
        print_submission_with_response("HTTP/1.1 400 Bad Request", r#"{"error":"token_revoked"}"#);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"plugin_token_revoked"}"#);
}

#[test]
fn empty_auth_token_is_rejected_before_network() {
    let artifact =
        std::env::temp_dir().join(format!("pandar-empty-token-{}.3mf", std::process::id()));
    write_artifact(&artifact, b"not empty");
    let artifact_path = artifact.to_string_lossy();
    let result = submit_print(b"http://127.0.0.1:9", b"   ", artifact_path.as_bytes());
    fs::remove_file(&artifact).unwrap();

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"invalid_auth_token"}"#);
}
