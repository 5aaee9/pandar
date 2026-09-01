use pandar_network_plugin::pandar_plugin_exchange_ticket;

use super::{body, exchange_ticket, one_shot_server};

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
fn exchange_ticket_rejects_empty_ticket_before_network() {
    let hub = b"http://127.0.0.1:9";
    let result = unsafe { pandar_plugin_exchange_ticket(hub.as_ptr(), hub.len(), b"".as_ptr(), 0) };

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"invalid_plugin_ticket"}"#);
}
