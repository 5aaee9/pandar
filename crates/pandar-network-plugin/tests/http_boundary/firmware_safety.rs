use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::{Duration, Instant},
};

use pandar_network_plugin::firmware::{FirmwarePluginSession, FirmwareSendOutcome, FirmwareTunnel};

const URL: &str = "https://user:secret@example.invalid/fw.bin?sig=REDIRECT_SENTINEL";
const PREPARED: &str =
    r#"{"command_id":"00000000-0000-0000-0000-000000000001","prepared_token":"prepared"}"#;

#[test]
fn firmware_http_execute_redirect_is_not_followed_or_replayed() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub = format!("http://{}", listener.local_addr().unwrap());
    let redirect = format!("{hub}/redirected");
    let server = thread::spawn(move || {
        let mut requests = Vec::new();
        let (mut prepare, _) = listener.accept().unwrap();
        requests.push(read_request(&mut prepare));
        respond(&mut prepare, "200 OK", &[], PREPARED);

        let (mut execute, _) = listener.accept().unwrap();
        requests.push(read_request(&mut execute));
        respond(
            &mut execute,
            "307 Temporary Redirect",
            &[("Location", redirect.as_str())],
            r#"{"error":"forged","phase":"pre_publish_failure"}"#,
        );

        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_millis(750);
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut redirected, _)) => {
                    redirected.set_nonblocking(false).unwrap();
                    requests.push(read_request(&mut redirected));
                    respond(&mut redirected, "200 OK", &[], "{}");
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::yield_now();
                }
                Err(error) => panic!("redirect listener failed: {error}"),
            }
        }
        requests
    });
    let session = FirmwarePluginSession::new(hub, "token".into(), 1);
    let mut diagnostics = Vec::new();

    let response = session.send_with_diagnostics(
        "SERIAL",
        "printer-1",
        &start_message(),
        FirmwareTunnel::Cloud,
        &mut diagnostics,
    );
    let requests = server.join().unwrap();

    assert_eq!(response.outcome, FirmwareSendOutcome::OutcomeUnknown);
    assert!(response.callback_token.is_none());
    assert_eq!(
        requests.len(),
        2,
        "execute redirect replayed the URL-bearing POST"
    );
    assert_eq!(
        requests
            .iter()
            .map(|request| request.matches(URL).count())
            .sum::<usize>(),
        1
    );
    let diagnostics = String::from_utf8_lossy(&diagnostics);
    assert!(diagnostics.contains("HTTP 307 Temporary Redirect"));
    assert!(!diagnostics.contains("REDIRECT_SENTINEL"));
    assert!(!diagnostics.contains("user:secret"));
}

#[test]
fn firmware_http_execute_5xx_pre_publish_phase_is_safe_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let mut requests = Vec::new();
        let (mut prepare, _) = listener.accept().unwrap();
        requests.push(read_request(&mut prepare));
        respond(&mut prepare, "200 OK", &[], PREPARED);
        let (mut execute, _) = listener.accept().unwrap();
        requests.push(read_request(&mut execute));
        respond(
            &mut execute,
            "500 Internal Server Error",
            &[],
            r#"{"error":"internal_server_error","phase":"pre_publish_failure"}"#,
        );

        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_millis(250);
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut retry, _)) => {
                    retry.set_nonblocking(false).unwrap();
                    requests.push(read_request(&mut retry));
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::yield_now();
                }
                Err(error) => panic!("execute retry listener failed: {error}"),
            }
        }
        requests
    });
    let session = FirmwarePluginSession::new(hub, "token".into(), 1);
    let mut diagnostics = Vec::new();

    let response = session.send_with_diagnostics(
        "SERIAL",
        "printer-1",
        &start_message(),
        FirmwareTunnel::Cloud,
        &mut diagnostics,
    );
    let requests = server.join().unwrap();

    assert_eq!(response.outcome, FirmwareSendOutcome::PrePublishFailure);
    assert!(response.callback_token.is_none());
    assert_eq!(requests.len(), 2, "typed pre-publish failure was retried");
    assert!(diagnostics.is_empty());
}

#[test]
fn firmware_http_execute_2xx_pre_publish_phase_is_safe_failure() {
    for body in [
        r#"{"command_id":"00000000-0000-0000-0000-000000000001","phase":"pre_publish_failure"}"#,
        r#"{"command_id":"00000000-0000-0000-0000-000000000001","phase":"pre_publish_failure","outcome":null,"transient_status":null,"error":"firmware_pre_publish_failure"}"#,
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let hub = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            let (mut prepare, _) = listener.accept().unwrap();
            requests.push(read_request(&mut prepare));
            respond(&mut prepare, "200 OK", &[], PREPARED);
            let (mut execute, _) = listener.accept().unwrap();
            requests.push(read_request(&mut execute));
            respond(&mut execute, "200 OK", &[], body);

            listener.set_nonblocking(true).unwrap();
            let deadline = Instant::now() + Duration::from_millis(250);
            while Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut retry, _)) => {
                        retry.set_nonblocking(false).unwrap();
                        requests.push(read_request(&mut retry));
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::yield_now();
                    }
                    Err(error) => panic!("execute retry listener failed: {error}"),
                }
            }
            requests
        });
        let session = FirmwarePluginSession::new(hub, "token".into(), 1);
        let mut diagnostics = Vec::new();

        let response = session.send_with_diagnostics(
            "SERIAL",
            "printer-1",
            &start_message(),
            FirmwareTunnel::Cloud,
            &mut diagnostics,
        );
        let requests = server.join().unwrap();

        assert_eq!(response.outcome, FirmwareSendOutcome::PrePublishFailure);
        assert!(response.callback_token.is_none());
        assert_eq!(requests.len(), 2, "typed pre-publish failure was retried");
        assert_eq!(
            requests
                .iter()
                .map(|request| request.matches(URL).count())
                .sum::<usize>(),
            1
        );
        assert!(diagnostics.is_empty());
    }
}

#[test]
fn firmware_http_malformed_4xx_preserves_status_and_decode_diagnostics() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut prepare, _) = listener.accept().unwrap();
        let _ = read_request(&mut prepare);
        respond(&mut prepare, "200 OK", &[], PREPARED);
        let (mut execute, _) = listener.accept().unwrap();
        let request = read_request(&mut execute);
        respond(&mut execute, "409 Conflict", &[], "not-json");
        request
    });
    let session = FirmwarePluginSession::new(hub, "token".into(), 1);
    let mut diagnostics = Vec::new();

    let response = session.send_with_diagnostics(
        "SERIAL",
        "printer-1",
        &start_message(),
        FirmwareTunnel::Cloud,
        &mut diagnostics,
    );
    let request = server.join().unwrap();

    assert_eq!(response.outcome, FirmwareSendOutcome::OutcomeUnknown);
    assert!(response.callback_token.is_none());
    assert_eq!(request.matches(URL).count(), 1);
    let diagnostics = String::from_utf8_lossy(&diagnostics);
    assert!(diagnostics.contains("HTTP 409 Conflict"));
    assert!(diagnostics.contains("decode Hub firmware execute error response"));
    assert!(!diagnostics.contains("not-json"));
    assert!(!diagnostics.contains("REDIRECT_SENTINEL"));
    assert!(!diagnostics.contains("user:secret"));
}

fn start_message() -> String {
    format!(
        r#"{{"upgrade":{{"command":"start","sequence_id":"9","src_id":1,"url":"{URL}","module":"ota","version":"1"}}}}"#
    )
}

fn read_request(stream: &mut TcpStream) -> String {
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
        .map(|value| value.parse::<usize>().unwrap())
        .unwrap_or(0);
    while request.len() - headers_end < content_length {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0);
        request.extend_from_slice(&buffer[..read]);
    }
    String::from_utf8(request).unwrap()
}

fn respond(stream: &mut TcpStream, status: &str, headers: &[(&str, &str)], body: &str) {
    let extra = headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect::<String>();
    let response = format!(
        "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).unwrap();
}
