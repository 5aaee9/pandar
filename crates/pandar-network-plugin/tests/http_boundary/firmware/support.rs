use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    thread,
};

use crate::support::read_http_request_with_timeout;

pub(super) const URL: &str = "https://user:secret@example.invalid/fw.bin?sig=SENTINEL";
pub(super) const PREPARED: &str =
    r#"{"command_id":"00000000-0000-0000-0000-000000000001","prepared_token":"prepared-1"}"#;

pub(super) fn start_message() -> String {
    format!(
        r#"{{"upgrade":{{"command":"start","sequence_id":"9001","src_id":1,"url":"{URL}","module":"ota","version":"01.02.03.04"}}}}"#
    )
}

pub(super) fn acknowledged(result: &str) -> String {
    acknowledged_with_phase("acknowledged", result)
}

pub(super) fn acknowledged_with_phase(phase: &str, result: &str) -> String {
    format!(
        r#"{{"command_id":"00000000-0000-0000-0000-000000000001","phase":"{phase}","outcome":{{"outcome":"acknowledged","acknowledgement":{{"command":"start","sequence_id":"9001","result":"{result}","err_code":17,"reason":"printer","message":"detail"}}}}}}"#
    )
}

pub(super) fn assert_redacted(diagnostics: &[u8]) {
    let diagnostics = String::from_utf8_lossy(diagnostics);
    assert!(
        !diagnostics.contains("SENTINEL"),
        "leaked URL query: {diagnostics}"
    );
    assert!(
        !diagnostics.contains("user:secret"),
        "leaked URL credentials: {diagnostics}"
    );
}

pub(super) enum Action {
    Json(&'static str, String),
    Drop,
}

impl Action {
    pub(super) fn json(status: &'static str, body: impl Into<String>) -> Self {
        Self::Json(status, body.into())
    }
}

pub(super) fn mock_hub(actions: Vec<Action>) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for action in actions {
            let (mut stream, _) = listener.accept().unwrap();
            requests.push(read_http_request_with_timeout(&mut stream, None));
            match action {
                Action::Json(status, body) => respond(&mut stream, status, &body),
                Action::Drop => {}
            }
        }
        requests
    });
    (url, handle)
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).unwrap();
}
