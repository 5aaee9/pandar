use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::Arc,
    thread,
    time::Duration,
};

use pandar_network_plugin::firmware::{
    FirmwareCallback, FirmwareCallbackQueue, FirmwarePluginSession, FirmwareTunnel,
};

#[test]
fn firmware_callback_windows_are_anchored_to_each_originating_handoff() {
    let queue = FirmwareCallbackQueue::new();
    let start = std::time::Instant::now();
    let delayed = queue
        .push(
            4,
            FirmwareCallback {
                dev_id: "DELAYED".into(),
                tunnel: FirmwareTunnel::Cloud,
                message: r#"{"upgrade":{"sequence_id":"one"}}"#.into(),
            },
        )
        .unwrap();
    let overlap = queue
        .push(
            4,
            FirmwareCallback {
                dev_id: "OVERLAP".into(),
                tunnel: FirmwareTunnel::Local,
                message: r#"{"upgrade":{"sequence_id":"two"}}"#.into(),
            },
        )
        .unwrap();

    assert!(queue.return_handoff_at(overlap, 200, 41, 73, start));
    assert!(
        queue
            .take_ready_at(start + Duration::from_millis(1_099))
            .is_none()
    );
    assert!(queue.return_handoff_at(delayed, 100, 0, 0, start + Duration::from_millis(500)));

    let ready = queue
        .take_ready_at(start + Duration::from_millis(1_100))
        .expect("overlapping callback becomes ready at its own handoff +1.1s");
    assert_eq!(ready.token, overlap);
    assert_eq!(ready.origin_tick, 200);
    assert_eq!(ready.local_generation, 41);
    assert_eq!(ready.cache_generation, 73);
    assert_eq!(ready.dev_id, "OVERLAP");
    assert_eq!(ready.tunnel, FirmwareTunnel::Local);
    assert_eq!(ready.message, r#"{"upgrade":{"sequence_id":"two"}}"#);
    assert!(
        queue
            .take_ready_at(start + Duration::from_millis(1_599))
            .is_none()
    );

    let ready = queue
        .take_ready_at(start + Duration::from_millis(1_600))
        .expect("delayed callback becomes ready at its own handoff +1.1s");
    assert_eq!(ready.token, delayed);
    assert_eq!(ready.origin_tick, 100);
    assert_eq!(ready.local_generation, 0);
    assert_eq!(ready.cache_generation, 0);
    assert_eq!(ready.dev_id, "DELAYED");
    assert_eq!(ready.tunnel, FirmwareTunnel::Cloud);
    assert_eq!(ready.message, r#"{"upgrade":{"sequence_id":"one"}}"#);
}

#[test]
fn firmware_callback_is_ineligible_before_handoff_and_expires_at_two_seconds() {
    let queue = FirmwareCallbackQueue::new();
    let start = std::time::Instant::now();
    let never_handed_off = queue
        .push(
            1,
            FirmwareCallback {
                dev_id: "A".into(),
                tunnel: FirmwareTunnel::Cloud,
                message: "never".into(),
            },
        )
        .unwrap();
    assert!(
        queue
            .take_ready_at(start + Duration::from_secs(60))
            .is_none()
    );
    assert!(queue.return_handoff_at(never_handed_off, 1, 0, 0, start));
    assert!(
        queue
            .take_ready_at(start + Duration::from_millis(2_000))
            .is_none()
    );
    assert!(!queue.return_handoff_at(never_handed_off, 2, 0, 0, start));
}

#[test]
fn firmware_callback_generation_cancellation_removes_only_matching_pending_entries() {
    let queue = FirmwareCallbackQueue::new();
    let start = std::time::Instant::now();
    let stale = queue
        .push(
            8,
            FirmwareCallback {
                dev_id: "STALE".into(),
                tunnel: FirmwareTunnel::Cloud,
                message: "stale".into(),
            },
        )
        .unwrap();
    let current = queue
        .push(
            9,
            FirmwareCallback {
                dev_id: "CURRENT".into(),
                tunnel: FirmwareTunnel::Local,
                message: "current".into(),
            },
        )
        .unwrap();
    queue.cancel_generation(8);

    assert!(!queue.return_handoff_at(stale, 1, 0, 0, start));
    assert!(queue.return_handoff_at(current, 2, 7, 83, start));
    let ready = queue
        .take_ready_at(start + Duration::from_millis(1_100))
        .unwrap();
    assert_eq!(ready.dev_id, "CURRENT");
    assert_eq!(ready.local_generation, 7);
    assert_eq!(ready.cache_generation, 83);
}

#[test]
fn firmware_callback_stop_wakes_blocked_consumer_and_joins_cleanly() {
    let queue = Arc::new(FirmwareCallbackQueue::new());
    let consumer_queue = Arc::clone(&queue);
    let consumer = thread::spawn(move || consumer_queue.wait_ready(Duration::from_secs(30)));
    thread::sleep(Duration::from_millis(25));

    queue.stop();

    assert!(consumer.join().unwrap().is_none());
    assert!(queue.is_stopped());
    assert!(
        queue
            .push(
                1,
                FirmwareCallback {
                    dev_id: "STOPPED".into(),
                    tunnel: FirmwareTunnel::Cloud,
                    message: "stopped".into(),
                },
            )
            .is_none()
    );
}

#[test]
fn firmware_callback_session_generation_change_cancels_pending_token() {
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
    let session = FirmwarePluginSession::new(hub.clone(), "token".into(), 1);
    let response = session.send(
        "SERIAL",
        "printer-1",
        r#"{"upgrade":{"command":"upgrade_confirm","sequence_id":"7","src_id":1}}"#,
        FirmwareTunnel::Cloud,
        1,
    );
    server.join().unwrap();
    let token = response.callback_token.unwrap();

    assert_eq!(session.sync_account(hub, "new-token".into()), 2);

    assert!(!session.return_handoff_at(token, 1, 0, 0, std::time::Instant::now()));
}

fn respond_once(listener: &TcpListener, body: &str) {
    let (mut stream, _) = listener.accept().unwrap();
    read_request(&mut stream);
    respond(&mut stream, body);
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

fn respond(stream: &mut TcpStream, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).unwrap();
}
