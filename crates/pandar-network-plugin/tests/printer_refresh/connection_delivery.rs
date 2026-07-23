use std::{
    ffi::c_void,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
};

use pandar_network_plugin::{
    PluginHttpResult, pandar_plugin_connection_claim_delivery, pandar_plugin_connection_refresh,
    pandar_plugin_connection_set_account_epoch, pandar_plugin_connection_take_transition,
    pandar_plugin_free_with_capacity, pandar_plugin_printer_refresh,
    pandar_plugin_printer_refresh_session_create, pandar_plugin_printer_refresh_session_destroy,
    pandar_plugin_printer_refresh_session_update,
};

const PRINTERS_RESPONSE: &str = r#"{"message":"success","devices":[{"dev_id":"serial-1","dev_name":"Printer","name":"Printer","dev_model_name":null,"model":null,"dev_online":false,"online":false,"task_status":"unknown","state":"unknown","gcode_state":null,"mc_percent":null,"mc_remaining_time":null,"layer_num":null,"total_layer_num":null,"task_id":null,"subtask_id":null,"gcode_file":null,"subtask_name":null,"hms":[],"pandar_printer_id":"printer-1","nozzle_temperatures":[],"active_nozzle":null,"bed_temperature_celsius":null,"bed_target_temperature_celsius":null,"chamber_temperature_celsius":null,"chamber_light_on":null,"materials":null}]}"#;

fn read_http_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0);
        request.extend_from_slice(&buffer[..read]);
    }
    String::from_utf8(request).unwrap()
}

fn spawn_server(
    responses: Vec<(&'static str, &'static str, &'static str)>,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub_url = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        for (path, status, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            assert!(request.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
            write!(
                stream,
                "{status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    (hub_url, server)
}

fn create_session(hub_url: &str) -> *mut c_void {
    pandar_plugin_printer_refresh_session_create(
        hub_url.as_ptr(),
        hub_url.len(),
        b"token".as_ptr(),
        b"token".len(),
    )
}

fn free_body(result: PluginHttpResult) {
    if !result.body_ptr.is_null() {
        pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap);
    }
}

fn refresh_printers(session: *mut c_void) -> PluginHttpResult {
    pandar_plugin_printer_refresh(session, std::ptr::null_mut(), None)
}

#[test]
fn unchanged_readyz_does_not_swallow_pending_printer_recovery_transition() {
    let (hub_url, server) = spawn_server(vec![
        (
            "/api/v1/plugin/printers",
            "HTTP/1.1 200 OK",
            PRINTERS_RESPONSE,
        ),
        ("/readyz", "HTTP/1.1 200 OK", r#"{"status":"ready"}"#),
    ]);
    let session = create_session(&hub_url);

    let printers = refresh_printers(session);
    assert_eq!(printers.status, 0);
    free_body(printers);
    let readiness = pandar_plugin_connection_refresh(session);
    assert_eq!(readiness.connected, 1);
    assert_eq!(readiness.changed, 0);
    assert_eq!(readiness.transition_ticket, 0);

    let pending = pandar_plugin_connection_take_transition(session);
    assert_eq!(pending.connected, 1);
    assert_eq!(pending.changed, 1);
    assert_ne!(pending.transition_ticket, 0);
    assert_eq!(
        pandar_plugin_connection_claim_delivery(session, pending.transition_ticket),
        1
    );
    assert_eq!(
        pandar_plugin_connection_claim_delivery(session, pending.transition_ticket),
        0
    );
    assert_eq!(pandar_plugin_connection_take_transition(session).changed, 0);

    pandar_plugin_printer_refresh_session_destroy(session);
    server.join().unwrap();
}

#[test]
fn authenticated_rejection_proves_hub_reachability_from_unknown_and_disconnected() {
    let (hub_url, server) = spawn_server(vec![
        (
            "/api/v1/plugin/printers",
            "HTTP/1.1 401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        ),
        (
            "/readyz",
            "HTTP/1.1 503 Service Unavailable",
            r#"{"status":"not_ready"}"#,
        ),
        (
            "/api/v1/plugin/printers",
            "HTTP/1.1 403 Forbidden",
            r#"{"error":"invalid_auth_token"}"#,
        ),
    ]);
    let session = create_session(&hub_url);

    let first = refresh_printers(session);
    assert_eq!(first.http_code, 401);
    free_body(first);
    let first_transition = pandar_plugin_connection_take_transition(session);
    assert_eq!(first_transition.connected, 1);
    assert_eq!(first_transition.changed, 1);
    assert_eq!(first_transition.auth_rejected, 1);
    assert_eq!(first_transition.auth_changed, 1);

    assert_eq!(pandar_plugin_connection_set_account_epoch(session, 1), 0);
    let disconnected = pandar_plugin_connection_refresh(session);
    assert_eq!(disconnected.connected, 0);
    assert_eq!(disconnected.changed, 1);
    let stale_disconnected = pandar_plugin_connection_take_transition(session);
    let second = refresh_printers(session);
    assert_eq!(second.http_code, 403);
    free_body(second);
    assert_eq!(
        pandar_plugin_connection_claim_delivery(session, stale_disconnected.transition_ticket),
        0
    );
    let second_transition = pandar_plugin_connection_take_transition(session);
    assert_eq!(second_transition.connected, 1);
    assert_eq!(second_transition.changed, 1);
    assert_eq!(second_transition.auth_rejected, 1);
    assert_eq!(second_transition.auth_changed, 1);

    pandar_plugin_printer_refresh_session_destroy(session);
    server.join().unwrap();
}

#[test]
fn newer_reachability_invalidates_old_ticket_and_claim_is_once_only() {
    let (hub_url, server) = spawn_server(vec![
        (
            "/readyz",
            "HTTP/1.1 503 Service Unavailable",
            r#"{"status":"not_ready"}"#,
        ),
        ("/readyz", "HTTP/1.1 200 OK", r#"{"status":"ready"}"#),
    ]);
    let session = create_session(&hub_url);

    let disconnected = pandar_plugin_connection_refresh(session);
    assert_eq!(disconnected.changed, 1);
    assert_eq!(disconnected.transition_ticket, 0);
    let old = pandar_plugin_connection_take_transition(session);
    assert_ne!(old.transition_ticket, 0);

    let connected = pandar_plugin_connection_refresh(session);
    assert_eq!(connected.changed, 1);
    assert_eq!(
        pandar_plugin_connection_claim_delivery(session, old.transition_ticket),
        0
    );
    let current = pandar_plugin_connection_take_transition(session);
    assert_ne!(current.transition_ticket, 0);
    assert_eq!(
        pandar_plugin_connection_claim_delivery(session, current.transition_ticket),
        1
    );
    assert_eq!(
        pandar_plugin_connection_claim_delivery(session, current.transition_ticket),
        0
    );

    pandar_plugin_printer_refresh_session_destroy(session);
    server.join().unwrap();
}

#[test]
fn account_change_invalidates_issued_reachability_and_auth_tickets() {
    let (hub_url, server) = spawn_server(vec![(
        "/api/v1/plugin/printers",
        "HTTP/1.1 401 Unauthorized",
        r#"{"error":"invalid_auth_token"}"#,
    )]);
    let session = create_session(&hub_url);

    let rejected = refresh_printers(session);
    assert_eq!(rejected.http_code, 401);
    free_body(rejected);
    let issued = pandar_plugin_connection_take_transition(session);
    assert_ne!(issued.transition_ticket, 0);
    assert_ne!(issued.auth_ticket, 0);

    assert_eq!(pandar_plugin_connection_set_account_epoch(session, 1), 0);
    assert_eq!(
        pandar_plugin_connection_claim_delivery(session, issued.transition_ticket),
        0
    );
    assert_eq!(
        pandar_plugin_connection_claim_delivery(session, issued.auth_ticket),
        0
    );

    pandar_plugin_printer_refresh_session_destroy(session);
    server.join().unwrap();
}

#[test]
fn token_rotation_preserves_pending_reachability_until_retry_cache_admission() {
    let (hub_url, server) = spawn_server(vec![
        (
            "/api/v1/plugin/printers",
            "HTTP/1.1 200 OK",
            PRINTERS_RESPONSE,
        ),
        (
            "/api/v1/plugin/printers",
            "HTTP/1.1 200 OK",
            PRINTERS_RESPONSE,
        ),
    ]);
    let session = create_session(&hub_url);

    let first = refresh_printers(session);
    assert_eq!(first.status, 0);
    free_body(first);
    assert_eq!(
        pandar_plugin_printer_refresh_session_update(
            session,
            hub_url.as_ptr(),
            hub_url.len(),
            b"new-token".as_ptr(),
            b"new-token".len(),
        ),
        0
    );
    let retry = refresh_printers(session);
    assert_eq!(retry.status, 0);
    free_body(retry);

    let pending = pandar_plugin_connection_take_transition(session);
    assert_eq!(pending.connected, 1);
    assert_eq!(pending.changed, 1);
    assert_ne!(pending.transition_ticket, 0);
    assert_eq!(
        pandar_plugin_connection_claim_delivery(session, pending.transition_ticket),
        1
    );

    pandar_plugin_printer_refresh_session_destroy(session);
    server.join().unwrap();
}
