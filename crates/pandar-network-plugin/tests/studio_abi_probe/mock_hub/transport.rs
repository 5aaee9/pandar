use crate::support::read_http_request_with_timeout;
use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

fn remaining(deadline: Instant, waiting_for: &str) -> Duration {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .unwrap_or_else(|| {
            panic!("timed out waiting for {waiting_for} before the Studio ABI probe deadline")
        })
}

pub(super) fn read_request_until(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    waiting_for: &str,
) -> Option<(TcpStream, String)> {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                let timeout = remaining(deadline, waiting_for);
                stream.set_read_timeout(Some(timeout)).unwrap();
                stream.set_write_timeout(Some(timeout)).unwrap();
                let mut stream = stream;
                let request = read_http_request_with_timeout(&mut stream, Some(timeout));
                return Some((stream, request));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if stop.load(Ordering::Acquire) {
                    return None;
                }
                assert!(
                    Instant::now() < deadline,
                    "timed out waiting for {waiting_for} before the Studio ABI probe deadline"
                );
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("failed accepting mock hub request: {error}"),
        }
    }
}

pub(super) fn write_response(stream: &mut std::net::TcpStream, status: &str, body: &str) {
    let response = format!(
        "{status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).unwrap();
}

pub(super) fn assert_request(request: &str, method: &str, path: &str, bearer: bool) {
    assert_request_with_token(request, method, path, bearer.then_some("probe-token"));
}

pub(super) fn assert_request_with_token(
    request: &str,
    method: &str,
    path: &str,
    bearer_token: Option<&str>,
) {
    assert!(
        request.starts_with(&format!("{method} {path} HTTP/1.1\r\n")),
        "unexpected request line: {request}"
    );
    if let Some(token) = bearer_token {
        assert!(
            request.contains(&format!("authorization: Bearer {token}")),
            "missing bearer auth: {request}"
        );
    }
}
