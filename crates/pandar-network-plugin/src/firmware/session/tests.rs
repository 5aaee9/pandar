use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

use crate::firmware::{FirmwarePluginSession, FirmwareTunnel, callbacks::test_hook};

#[test]
fn firmware_callback_generation_update_cannot_race_between_validation_and_enqueue() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        respond_once(
            &listener,
            r#"{"command_id":"00000000-0000-0000-0000-000000000001","prepared_token":"prepared"}"#,
        );
        respond_once(
            &listener,
            r#"{"command_id":"00000000-0000-0000-0000-000000000001","phase":"acknowledged","outcome":{"outcome":"acknowledged","acknowledgement":{"command":"upgrade_confirm","sequence_id":"7","result":"success"}}}"#,
        );
    });
    let session = Arc::new(FirmwarePluginSession::new(hub.clone(), "token".into(), 1));
    test_hook::arm();
    let sending = Arc::clone(&session);
    let sender = thread::spawn(move || {
        sending.send(
            "SERIAL",
            "printer-1",
            r#"{"upgrade":{"command":"upgrade_confirm","sequence_id":"7","src_id":1}}"#,
            FirmwareTunnel::Cloud,
        )
    });
    test_hook::wait_until_reached();

    let updating = Arc::clone(&session);
    let (updated_tx, updated_rx) = mpsc::channel();
    let updater = thread::spawn(move || {
        updating.update(hub, "new-token".into(), 2);
        updated_tx.send(()).unwrap();
    });
    let updated_before_release = updated_rx.recv_timeout(Duration::from_secs(1)).is_ok();
    test_hook::release();
    let response = sender.join().unwrap();
    if !updated_before_release {
        updated_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    }
    updater.join().unwrap();
    server.join().unwrap();

    let token = response.callback_token.expect("acknowledgement token");
    assert!(
        !session.return_handoff_at(token, 1, std::time::Instant::now()),
        "stale callback escaped generation cancellation"
    );
}

#[test]
fn firmware_callback_generation_cancel_cannot_race_with_in_flight_enqueue() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        respond_once(
            &listener,
            r#"{"command_id":"00000000-0000-0000-0000-000000000001","prepared_token":"prepared"}"#,
        );
        respond_once(
            &listener,
            r#"{"command_id":"00000000-0000-0000-0000-000000000001","phase":"acknowledged","outcome":{"outcome":"acknowledged","acknowledgement":{"command":"upgrade_confirm","sequence_id":"8","result":"success"}}}"#,
        );
    });
    let session = Arc::new(FirmwarePluginSession::new(hub, "token".into(), 1));
    test_hook::arm();
    let sending = Arc::clone(&session);
    let sender = thread::spawn(move || {
        sending.send(
            "SERIAL",
            "printer-1",
            r#"{"upgrade":{"command":"upgrade_confirm","sequence_id":"8","src_id":1}}"#,
            FirmwareTunnel::Cloud,
        )
    });
    test_hook::wait_until_reached();

    let cancelling = Arc::clone(&session);
    let (cancelled_tx, cancelled_rx) = mpsc::channel();
    let canceller = thread::spawn(move || {
        cancelling.cancel_generation(1);
        cancelled_tx.send(()).unwrap();
    });
    let cancelled_before_release = cancelled_rx.recv_timeout(Duration::from_secs(1)).is_ok();
    test_hook::release();
    let response = sender.join().unwrap();
    if !cancelled_before_release {
        cancelled_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    }
    canceller.join().unwrap();
    server.join().unwrap();

    let token = response.callback_token.expect("acknowledgement token");
    assert!(
        !session.return_handoff_at(token, 1, std::time::Instant::now()),
        "stale callback escaped explicit generation cancellation"
    );
}

fn respond_once(listener: &TcpListener, body: &str) {
    let (mut stream, _) = listener.accept().unwrap();
    read_request(&mut stream);
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).unwrap();
}

fn read_request(stream: &mut TcpStream) {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    let headers_end = loop {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0);
        request.extend_from_slice(&buffer[..read]);
        if let Some(position) = request.windows(4).position(|value| value == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let headers = String::from_utf8_lossy(&request[..headers_end]);
    let content_length = headers
        .lines()
        .find_map(|line| line.strip_prefix("content-length: "))
        .unwrap()
        .parse::<usize>()
        .unwrap();
    while request.len() - headers_end < content_length {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0);
        request.extend_from_slice(&buffer[..read]);
    }
}
