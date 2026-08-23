//! Printer-events WebSocket endpoint for the Studio ABI probe mock hub.
//!
//! The plugin core holds a persistent account-scoped cache fed by the Hub
//! stream `GET /api/v1/tenants/{tenant}/printer-events?projection=studio&version=1`.
//! Mock modes accept that upgrade through [`next_incoming`] and drive cache
//! state by sending snapshot/upsert/removal frames over the returned channel
//! instead of answering `GET /api/v1/plugin/printers`.

use std::{
    io::Write,
    net::TcpStream,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use crate::support::read_http_request_with_timeout;

/// One accepted hub connection: either a plain HTTP request or the
/// printer-events WebSocket upgrade awaiting a serve/reject decision.
pub(crate) enum Incoming {
    Http(TcpStream, String),
    Stream(StreamUpgrade),
}

pub(crate) struct StreamUpgrade {
    pub(crate) stream: TcpStream,
    pub(crate) request: String,
}

impl StreamUpgrade {
    /// Completes the 101 handshake on a detached thread and returns the
    /// frame channel for driving snapshot/upsert/removal events.
    pub(crate) fn serve(self) -> mpsc::Sender<String> {
        let (tx, rx) = mpsc::channel::<String>();
        let Self { stream, request } = self;
        thread::spawn(move || serve_stream(stream, &request, rx));
        tx
    }

    /// Answers the upgrade with a plain HTTP error (e.g. 401/403 auth
    /// rejection) and closes the socket.
    pub(crate) fn reject(mut self, status: &str, body: &str) {
        let response = format!(
            "{status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = self.stream.write_all(response.as_bytes());
    }
}

const PRINTER_EVENTS_PREFIX: &str = "GET /api/v1/tenants/";

fn is_printer_events_upgrade(request: &str) -> bool {
    let request_line = request.lines().next().unwrap_or_default();
    let upgrade_header = request.lines().any(|line| {
        line.split_once(':')
            .is_some_and(|(name, _value)| name.eq_ignore_ascii_case("upgrade"))
    });
    request_line.starts_with(PRINTER_EVENTS_PREFIX)
        && request_line.contains("/printer-events?")
        && request_line.contains("projection=studio")
        && request_line.contains("version=1")
        && upgrade_header
}

/// Accepts connections until the deadline. WebSocket upgrades are returned
/// unresolved; HTTP requests are returned with their raw request text.
pub(super) fn next_incoming(
    listener: &std::net::TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
) -> Incoming {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                let timeout = deadline
                    .checked_duration_since(Instant::now())
                    .filter(|remaining| !remaining.is_zero());
                stream.set_read_timeout(timeout).unwrap();
                stream.set_write_timeout(timeout).unwrap();
                let mut stream = stream;
                let mut first = [0_u8; 1];
                match stream.peek(&mut first) {
                    Ok(0) => continue,
                    Ok(_) => {}
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::UnexpectedEof
                        ) =>
                    {
                        continue;
                    }
                    Err(error) => panic!("mock hub failed waiting for request bytes: {error}"),
                }
                let request = read_http_request_with_timeout(&mut stream, timeout);
                if is_printer_events_upgrade(&request) {
                    return Incoming::Stream(StreamUpgrade { stream, request });
                }
                return Incoming::Http(stream, request);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if stop.load(Ordering::Acquire) {
                    panic!("mock hub stopped while waiting for a connection");
                }
                assert!(
                    Instant::now() < deadline,
                    "timed out waiting for a mock hub connection"
                );
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("failed accepting mock hub request: {error}"),
        }
    }
}

/// Upgrades the socket to a WebSocket and serves scripted frames plus hub
/// pings until the peer disconnects, `@close` is queued, or the socket dies.
///
/// The upgrade request bytes were already consumed by the accept loop, so the
/// 101 handshake is written directly instead of via `accept_hdr`.
fn serve_stream(mut stream: TcpStream, request: &str, frames: mpsc::Receiver<String>) {
    let key = request.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("sec-websocket-key")
            .then(|| value.trim().to_owned())
    });
    let Some(key) = key else {
        eprintln!("pandar mock hub stream upgrade lacked a WebSocket key");
        return;
    };
    let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
    let handshake = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Connection: Upgrade\r\n\
         Upgrade: websocket\r\n\
         Sec-WebSocket-Accept: {accept}\r\n\
         \r\n"
    );
    if stream.write_all(handshake.as_bytes()).is_err() {
        return;
    }
    let mut ws =
        tungstenite::WebSocket::from_raw_socket(stream, tungstenite::protocol::Role::Server, None);
    let _ = ws.get_ref().set_nonblocking(true);
    let mut next_ping = Instant::now() + Duration::from_secs(20);
    let mut pong_deadline = None;
    loop {
        while let Ok(frame) = frames.try_recv() {
            if frame == "@close" {
                let _ = ws.close(None);
                let _ = ws.flush();
                let _ = ws.get_ref().shutdown(std::net::Shutdown::Both);
                return;
            }
            if ws.write(tungstenite::Message::text(frame)).is_err() {
                return;
            }
            let _ = ws.flush();
        }
        if let Ok(message) = ws.read() {
            if matches!(message, tungstenite::Message::Pong(_)) {
                pong_deadline = None;
            }
            let _ = ws.flush();
        }
        if pong_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return;
        }
        if Instant::now() >= next_ping {
            if ws
                .write(tungstenite::Message::Ping(Vec::new().into()))
                .is_err()
            {
                return;
            }
            let _ = ws.flush();
            pong_deadline = Some(Instant::now() + Duration::from_secs(10));
            next_ping = Instant::now() + Duration::from_secs(20);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

// ---- frame helpers ---------------------------------------------------------

/// A complete initial snapshot: begin, one upsert per device, end.
pub(crate) fn snapshot_script(devices: &[&str]) -> Vec<String> {
    let mut frames = vec![r#"{"type":"snapshot_begin","version":1}"#.to_owned()];
    frames.extend(devices.iter().map(|device| upsert_frame(device)));
    frames.push(r#"{"type":"snapshot_end","version":1}"#.to_owned());
    frames
}

pub(crate) fn upsert_frame(device: &str) -> String {
    format!(r#"{{"type":"printer_upsert","printer":{device}}}"#)
}
