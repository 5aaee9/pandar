use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::{Duration, Instant},
};

use pandar_network_plugin::firmware::{
    FirmwarePluginSession, FirmwareSendOutcome, FirmwareTunnel, PLUGIN_JSON_BODY_LIMIT,
};

use crate::support::{read_http_request_with_timeout, request_body};

use super::support::{Action, mock_hub};

#[test]
fn firmware_http_catalog_uses_typed_state_and_exact_studio_envelope() {
    let response = r#"{
        "firmware":{"module_revision":0,"status_revision":0},
        "catalog":[
            {"target":"printer","version":"1","url":"printer.bin","description":"main"},
            {"target":"ams","version":"2","url":"ams.bin","description":"ams"},
            {"target":"ams","version":"3","url":"","description":"hidden"}
        ]
    }"#;
    let (hub, server) = mock_hub(vec![Action::json("200 OK", response)]);
    let session = FirmwarePluginSession::new(hub, "token".into(), 1);

    let catalog: serde_json::Value =
        serde_json::from_str(&session.catalog_json("SERIAL", "printer-1").unwrap()).unwrap();
    let requests = server.join().unwrap();

    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /api/v1/plugin/printers/printer-1/firmware "));
    assert_eq!(catalog["devices"][0]["firmware"][0]["url"], "printer.bin");
    assert_eq!(
        catalog["devices"][0]["ams"][0]["firmware"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn firmware_http_live_refresh_preserves_sequence_and_order_or_renders_typed_failure() {
    let response = r#"{
        "command_id":"00000000-0000-0000-0000-000000000001",
        "modules":[
            {"name":"n3s/0","sw_ver":"2"},
            {"name":"ota","sw_ver":"1","hw_ver":"AP05","flag":5}
        ],
        "module_revision":8
    }"#;
    let (hub, server) = mock_hub(vec![Action::json("200 OK", response)]);
    let session = FirmwarePluginSession::new(hub, "token".into(), 1);
    let success: serde_json::Value =
        serde_json::from_str(&session.refresh_version_json("printer-1", "0009")).unwrap();
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("POST /api/v1/plugin/printers/printer-1/firmware/refresh "));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(request_body(&requests[0])).unwrap(),
        serde_json::json!({"sequence_id":"0009"})
    );
    assert_eq!(success["info"]["sequence_id"], "0009");
    assert_eq!(success["info"]["module"][0]["name"], "n3s/0");
    assert!(success["info"].get("result").is_none());

    let (hub, server) = mock_hub(vec![Action::json(
        "502 Bad Gateway",
        r#"{"error":"firmware_refresh_failed"}"#,
    )]);
    let session = FirmwarePluginSession::new(hub, "token".into(), 1);
    let failure: serde_json::Value =
        serde_json::from_str(&session.refresh_version_json("printer-1", "0009")).unwrap();
    assert_eq!(server.join().unwrap().len(), 1);
    assert_eq!(
        failure,
        serde_json::json!({
            "info":{"command":"get_version","sequence_id":"0009","result":"fail","module":[]}
        })
    );

    let empty_response = r#"{
        "command_id":"00000000-0000-0000-0000-000000000002",
        "modules":[],
        "module_revision":9
    }"#;
    let (hub, server) = mock_hub(vec![Action::json("200 OK", empty_response)]);
    let session = FirmwarePluginSession::new(hub, "token".into(), 1);
    let empty: serde_json::Value =
        serde_json::from_str(&session.refresh_version_json("printer-1", "empty-001")).unwrap();
    assert_eq!(server.join().unwrap().len(), 1);
    assert_eq!(
        empty,
        serde_json::json!({
            "info":{"command":"get_version","sequence_id":"empty-001","result":"fail","module":[]}
        })
    );
}

#[test]
fn firmware_http_oversized_input_is_rejected_before_hub_contact() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let hub = format!("http://{}", listener.local_addr().unwrap());
    let session = FirmwarePluginSession::new(hub, "token".into(), 1);
    let oversized = format!(
        r#"{{"upgrade":{{"command":"upgrade_confirm","sequence_id":"{}","src_id":1}}}}"#,
        "x".repeat(PLUGIN_JSON_BODY_LIMIT)
    );

    let result = session.send("SERIAL", "printer-1", &oversized, FirmwareTunnel::Cloud);

    assert_eq!(result.outcome, FirmwareSendOutcome::PrePublishFailure);
    assert!(listener.accept().is_err(), "oversized input contacted Hub");
}

#[test]
fn firmware_http_rejects_declared_oversized_response_before_reading_body() {
    const OVERSIZED_LENGTH: usize = 64 * 1024 + 1;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_http_request_with_timeout(&mut stream, None);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {OVERSIZED_LENGTH}\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        stream.flush().unwrap();
        observe_client_disconnect(stream)
    });
    let session = FirmwarePluginSession::new(hub, "token".into(), 1);
    let started = Instant::now();

    let error = session.catalog_json("SERIAL", "printer-1").unwrap_err();
    let client_elapsed = started.elapsed();
    let server_elapsed = server.join().unwrap();

    assert!(
        format!("{error:#}").contains("firmware catalog response exceeded body limit"),
        "unexpected catalog error: {error:#}"
    );
    assert_rejected_before_http_timeout(client_elapsed, server_elapsed);
}

#[test]
fn firmware_http_rejects_chunked_oversized_response_before_terminal_chunk() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_http_request_with_timeout(&mut stream, None);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        write!(stream, "{:X}\r\n", 64 * 1024).unwrap();
        stream.write_all(&vec![b'x'; 64 * 1024]).unwrap();
        stream.write_all(b"\r\n1\r\nx\r\n").unwrap();
        stream.flush().unwrap();
        observe_client_disconnect(stream)
    });
    let session = FirmwarePluginSession::new(hub, "token".into(), 1);
    let started = Instant::now();

    let error = session.catalog_json("SERIAL", "printer-1").unwrap_err();
    let client_elapsed = started.elapsed();
    let server_elapsed = server.join().unwrap();

    assert!(
        format!("{error:#}").contains("firmware catalog response exceeded body limit"),
        "unexpected catalog error: {error:#}"
    );
    assert_rejected_before_http_timeout(client_elapsed, server_elapsed);
}

fn observe_client_disconnect(mut stream: TcpStream) -> Duration {
    stream
        .set_read_timeout(Some(Duration::from_secs(6)))
        .unwrap();
    let started = Instant::now();
    let mut byte = [0_u8; 1];
    match stream.read(&mut byte) {
        Ok(0) => started.elapsed(),
        Ok(read) => panic!("client sent {read} unexpected bytes after its request"),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::BrokenPipe
            ) =>
        {
            started.elapsed()
        }
        Err(error) => panic!("client did not disconnect after oversized response: {error}"),
    }
}

fn assert_rejected_before_http_timeout(client_elapsed: Duration, server_elapsed: Duration) {
    let deadline = Duration::from_secs(4);
    assert!(
        client_elapsed < deadline,
        "client buffered or waited for the unfinished oversized body for {client_elapsed:?}"
    );
    assert!(
        server_elapsed < deadline,
        "server did not observe an early client disconnect for {server_elapsed:?}"
    );
}
