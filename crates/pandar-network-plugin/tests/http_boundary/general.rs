use pandar_network_plugin::pandar_plugin_exchange_ticket;

use super::{TOKEN, body, exchange_ticket, get_jobs, one_shot_server};

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
    let result = get_jobs(b"http://127.0.0.1:9", b"   ");

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"invalid_auth_token"}"#);
}

#[test]
fn authenticated_401_maps_to_invalid_auth_token() {
    let hub_url = one_shot_server(
        "GET",
        "/api/v1/plugin/jobs",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 401 Unauthorized",
        r#"{"error":"secret token"}"#,
        None,
    );
    let result = get_jobs(hub_url.as_bytes(), TOKEN);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 401);
    assert_eq!(body(result), r#"{"error":"invalid_auth_token"}"#);
}

#[test]
fn forbidden_maps_to_plugin_forbidden() {
    let hub_url = one_shot_server(
        "GET",
        "/api/v1/plugin/jobs",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 403 Forbidden",
        r#"{"error":"tenant xyz"}"#,
        None,
    );
    let result = get_jobs(hub_url.as_bytes(), TOKEN);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 403);
    assert_eq!(body(result), r#"{"error":"plugin_forbidden"}"#);
}

#[test]
fn not_found_without_stable_code_maps_to_printer_not_found() {
    let hub_url = one_shot_server(
        "GET",
        "/api/v1/plugin/jobs",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 404 Not Found",
        r#"{"error":"missing /tmp/x"}"#,
        None,
    );
    let result = get_jobs(hub_url.as_bytes(), TOKEN);

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
fn token_revoked_body_maps_to_plugin_token_revoked() {
    let hub_url = one_shot_server(
        "GET",
        "/api/v1/plugin/jobs",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 400 Bad Request",
        r#"{"error":"token_revoked"}"#,
        None,
    );
    let result = get_jobs(hub_url.as_bytes(), TOKEN);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"plugin_token_revoked"}"#);
}

#[test]
fn unrecognized_server_error_maps_to_invalid_response() {
    let hub_url = one_shot_server(
        "GET",
        "/api/v1/plugin/jobs",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 500 Internal Server Error",
        r#"{"error":"db password"}"#,
        None,
    );
    let result = get_jobs(hub_url.as_bytes(), TOKEN);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 500);
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
