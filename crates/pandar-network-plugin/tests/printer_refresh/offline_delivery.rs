use std::{
    ffi::c_void,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread,
};

use pandar_network_plugin::{
    PluginHttpResult, pandar_plugin_connection_claim_delivery,
    pandar_plugin_connection_set_account_epoch, pandar_plugin_connection_take_offline,
    pandar_plugin_free_with_capacity, pandar_plugin_printer_refresh,
    pandar_plugin_printer_refresh_session_create, pandar_plugin_printer_refresh_session_destroy,
    pandar_plugin_printer_refresh_session_update,
};
use serde_json::{Value, json};

const PRINTER_TEMPLATE: &str = r#"{"message":"success","devices":[{"dev_id":"serial-1","dev_name":"Printer","name":"Printer","dev_model_name":null,"model":null,"dev_online":false,"online":false,"task_status":"unknown","state":"unknown","gcode_state":null,"mc_percent":null,"mc_remaining_time":null,"layer_num":null,"total_layer_num":null,"task_id":null,"subtask_id":null,"gcode_file":null,"subtask_name":null,"hms":[],"pandar_printer_id":"printer-1","nozzle_temperatures":[],"active_nozzle":null,"bed_temperature_celsius":null,"bed_target_temperature_celsius":null,"chamber_temperature_celsius":null,"chamber_light_on":null,"materials":null}]}"#;

fn printer_response(states: &[(&str, bool)]) -> String {
    let mut response = serde_json::from_str::<Value>(PRINTER_TEMPLATE).unwrap();
    let template = response["devices"][0].clone();
    response["devices"] = Value::Array(
        states
            .iter()
            .map(|(dev_id, online)| {
                let mut device = template.clone();
                device["dev_id"] = json!(dev_id);
                device["pandar_printer_id"] = json!(format!("printer-{dev_id}"));
                device["dev_online"] = json!(online);
                device["online"] = json!(online);
                device
            })
            .collect(),
    );
    serde_json::to_string(&response).unwrap()
}

fn read_http_request(stream: &mut TcpStream) {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0);
        request.extend_from_slice(&buffer[..read]);
    }
    assert!(
        String::from_utf8(request)
            .unwrap()
            .starts_with("GET /api/v1/plugin/printers HTTP/1.1\r\n")
    );
}

fn write_response(stream: &mut TcpStream, body: &str) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

fn spawn_server(bodies: Vec<String>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub_url = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        for body in bodies {
            let (mut stream, _) = listener.accept().unwrap();
            read_http_request(&mut stream);
            write_response(&mut stream, &body);
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

fn refresh(session: *mut c_void) {
    let result = pandar_plugin_printer_refresh(session, std::ptr::null_mut(), None);
    assert_eq!(result.status, 0);
    free_body(result);
}

fn update_token(session: *mut c_void, hub_url: &str) {
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
}

fn free_body(result: PluginHttpResult) {
    if !result.body_ptr.is_null() {
        pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap);
    }
}

extern "C" fn collect_offline(
    context: *mut c_void,
    dev_id: *const u8,
    dev_id_len: usize,
    ticket: u64,
) {
    let deliveries = unsafe { &mut *context.cast::<Vec<(String, u64)>>() };
    let dev_id = unsafe { std::slice::from_raw_parts(dev_id, dev_id_len) };
    deliveries.push((String::from_utf8(dev_id.to_vec()).unwrap(), ticket));
}

fn take_offline(session: *mut c_void) -> Vec<(String, u64)> {
    let mut deliveries = Vec::new();
    assert_eq!(
        pandar_plugin_connection_take_offline(
            session,
            (&mut deliveries as *mut Vec<(String, u64)>).cast(),
            Some(collect_offline),
        ),
        0
    );
    deliveries
}

#[test]
fn online_recovery_invalidates_only_that_printers_offline_ticket() {
    let (hub_url, server) = spawn_server(vec![
        printer_response(&[("a", true), ("b", true)]),
        printer_response(&[("a", false), ("b", false)]),
        printer_response(&[("a", true), ("b", false)]),
    ]);
    let session = create_session(&hub_url);

    refresh(session);
    refresh(session);
    let offline = take_offline(session);
    let a_ticket = offline.iter().find(|(dev_id, _)| dev_id == "a").unwrap().1;
    let b_ticket = offline.iter().find(|(dev_id, _)| dev_id == "b").unwrap().1;

    refresh(session);
    assert_eq!(
        pandar_plugin_connection_claim_delivery(session, a_ticket),
        0
    );
    assert_eq!(
        pandar_plugin_connection_claim_delivery(session, b_ticket),
        1
    );
    assert_eq!(
        pandar_plugin_connection_claim_delivery(session, b_ticket),
        0
    );

    pandar_plugin_printer_refresh_session_destroy(session);
    server.join().unwrap();
}

#[test]
fn beginning_refresh_does_not_cancel_issued_offline_ticket() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub_url = format!("http://{}", listener.local_addr().unwrap());
    let online = printer_response(&[("a", true)]);
    let offline = printer_response(&[("a", false)]);
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        for body in [&online, &offline] {
            let (mut stream, _) = listener.accept().unwrap();
            read_http_request(&mut stream);
            write_response(&mut stream, body);
        }
        let (mut stream, _) = listener.accept().unwrap();
        read_http_request(&mut stream);
        started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        write_response(&mut stream, &offline);
    });
    let session = create_session(&hub_url);

    refresh(session);
    refresh(session);
    let ticket = take_offline(session)[0].1;
    let session_address = session as usize;
    let in_flight = thread::spawn(move || refresh(session_address as *mut c_void));
    started_rx.recv().unwrap();

    assert_eq!(pandar_plugin_connection_claim_delivery(session, ticket), 1);
    release_tx.send(()).unwrap();
    in_flight.join().unwrap();

    pandar_plugin_printer_refresh_session_destroy(session);
    server.join().unwrap();
}

#[test]
fn same_hub_token_rotation_reports_printers_that_retry_confirms_offline_or_removed() {
    let (hub_url, server) = spawn_server(vec![
        printer_response(&[("a", true), ("b", true)]),
        printer_response(&[("a", false)]),
    ]);
    let session = create_session(&hub_url);

    refresh(session);
    update_token(session, &hub_url);
    refresh(session);

    let offline = take_offline(session);
    assert_eq!(
        offline
            .iter()
            .map(|(dev_id, _)| dev_id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );

    pandar_plugin_printer_refresh_session_destroy(session);
    server.join().unwrap();
}

#[test]
fn same_hub_token_rotation_reports_unconfirmed_printer_after_non_auth_failure_once() {
    let (hub_url, server) = spawn_server(vec![
        printer_response(&[("a", true)]),
        r#"{"message":"broken"}"#.to_owned(),
        r#"{"message":"still-broken"}"#.to_owned(),
    ]);
    let session = create_session(&hub_url);

    refresh(session);
    update_token(session, &hub_url);
    let failed = pandar_plugin_printer_refresh(session, std::ptr::null_mut(), None);
    assert_ne!(failed.status, 0);
    free_body(failed);
    let ticket = take_offline(session)[0].1;

    let repeated = pandar_plugin_printer_refresh(session, std::ptr::null_mut(), None);
    assert_ne!(repeated.status, 0);
    free_body(repeated);
    assert!(take_offline(session).is_empty());
    assert_eq!(pandar_plugin_connection_claim_delivery(session, ticket), 1);

    pandar_plugin_printer_refresh_session_destroy(session);
    server.join().unwrap();
}

#[test]
fn same_hub_token_rotation_preserves_offline_tickets_until_online_recovery() {
    let (hub_url, server) = spawn_server(vec![
        printer_response(&[("a", true), ("b", true)]),
        r#"{"message":"broken"}"#.to_owned(),
        printer_response(&[("a", true), ("b", true)]),
    ]);
    let session = create_session(&hub_url);

    refresh(session);
    let failed = pandar_plugin_printer_refresh(session, std::ptr::null_mut(), None);
    assert_ne!(failed.status, 0);
    free_body(failed);
    let offline = take_offline(session);
    let a_ticket = offline.iter().find(|(dev_id, _)| dev_id == "a").unwrap().1;
    let b_ticket = offline.iter().find(|(dev_id, _)| dev_id == "b").unwrap().1;

    update_token(session, &hub_url);
    assert_eq!(
        pandar_plugin_connection_claim_delivery(session, a_ticket),
        1
    );
    refresh(session);
    assert_eq!(
        pandar_plugin_connection_claim_delivery(session, b_ticket),
        0
    );
    assert!(take_offline(session).is_empty());

    pandar_plugin_printer_refresh_session_destroy(session);
    server.join().unwrap();
}

#[test]
fn account_change_invalidates_issued_offline_ticket() {
    let (hub_url, server) = spawn_server(vec![
        printer_response(&[("a", true)]),
        printer_response(&[("a", false)]),
    ]);
    let session = create_session(&hub_url);

    refresh(session);
    refresh(session);
    let ticket = take_offline(session)[0].1;

    assert_eq!(pandar_plugin_connection_set_account_epoch(session, 1), 0);
    assert_eq!(pandar_plugin_connection_claim_delivery(session, ticket), 0);

    pandar_plugin_printer_refresh_session_destroy(session);
    server.join().unwrap();
}
