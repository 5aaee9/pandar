use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::{Duration, Instant},
};

const SERVER_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;

pub struct PrintSink {
    pub url: String,
    thread: thread::JoinHandle<Result<Vec<u8>, String>>,
}

impl PrintSink {
    pub fn spawn() -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|error| format!("bind contract print sink: {error}"))?;
        let address = listener
            .local_addr()
            .map_err(|error| format!("read contract print sink address: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("configure contract print sink: {error}"))?;
        let thread = thread::spawn(move || serve_until_print(listener));
        Ok(Self {
            url: format!("http://{address}"),
            thread,
        })
    }

    pub fn finish(self) -> Result<Vec<u8>, String> {
        self.thread
            .join()
            .map_err(|_| "contract print sink panicked".to_owned())?
    }
}

fn serve_until_print(listener: TcpListener) -> Result<Vec<u8>, String> {
    let deadline = Instant::now() + SERVER_TIMEOUT;
    let mut print_request = None;
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .map_err(|error| format!("configure accepted print sink socket: {error}"))?;
                let request = read_request(&mut stream)?;
                if is_printer_events_upgrade(&request) {
                    thread::spawn(move || {
                        if let Err(error) = serve_printer_events(stream, &request) {
                            eprintln!("contract printer-event stream failed: {error}");
                        }
                    });
                    continue;
                }
                let is_print = request.starts_with(b"POST /api/v1/plugin/prints ");
                if is_print {
                    print_request = Some(request);
                    respond(
                        &mut stream,
                        "201 Created",
                        r#"{"task_id":41,"studio_submission_id":41,"status":"queued"}"#,
                    )?;
                } else if request.starts_with(b"GET /api/v1/plugin/printers ") {
                    respond(&mut stream, "200 OK", printer_response())?;
                } else if request.starts_with(b"GET /api/v1/plugin/jobs/41 ") {
                    respond(
                        &mut stream,
                        "200 OK",
                        r#"{"studio_submission_id":41,"job_status":"succeeded","print_status":"completed"}"#,
                    )?;
                    if let Some(request) = print_request {
                        return Ok(request);
                    }
                } else if request.starts_with(b"GET /readyz ") {
                    respond(&mut stream, "200 OK", r#"{"status":"ok"}"#)?;
                } else {
                    respond(&mut stream, "404 Not Found", r#"{"error":"not_found"}"#)?;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err("contract print sink did not receive print submission".to_owned());
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(format!("accept contract print request: {error}")),
        }
    }
}

fn is_printer_events_upgrade(request: &[u8]) -> bool {
    let request = String::from_utf8_lossy(request);
    let request_line = request.lines().next().unwrap_or_default();
    let upgrade_header = request.lines().any(|line| {
        line.split_once(':')
            .is_some_and(|(name, _)| name.eq_ignore_ascii_case("upgrade"))
    });
    request_line
        == "GET /api/v1/tenants/contract-tenant/printer-events?projection=studio&version=1 HTTP/1.1"
        && upgrade_header
}

fn serve_printer_events(mut stream: TcpStream, request: &[u8]) -> Result<(), String> {
    let request = String::from_utf8_lossy(request);
    let key = request.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("sec-websocket-key")
            .then(|| value.trim().to_owned())
    });
    let key = key.ok_or("contract printer-event upgrade omitted Sec-WebSocket-Key")?;
    let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
    let handshake = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Connection: Upgrade\r\n\
         Upgrade: websocket\r\n\
         Sec-WebSocket-Accept: {accept}\r\n\
         \r\n"
    );
    stream
        .write_all(handshake.as_bytes())
        .map_err(|error| format!("write contract printer-event handshake: {error}"))?;
    let mut socket =
        tungstenite::WebSocket::from_raw_socket(stream, tungstenite::protocol::Role::Server, None);
    for frame in printer_snapshot_frames() {
        socket
            .write(tungstenite::Message::text(frame))
            .map_err(|error| format!("write contract printer-event snapshot: {error}"))?;
    }
    socket
        .flush()
        .map_err(|error| format!("flush contract printer-event snapshot: {error}"))?;
    socket
        .get_mut()
        .set_nonblocking(true)
        .map_err(|error| format!("configure contract printer-event stream: {error}"))?;
    loop {
        match socket.read() {
            Ok(_) => {
                socket
                    .flush()
                    .map_err(|error| format!("flush contract printer-event response: {error}"))?;
            }
            Err(tungstenite::Error::Io(error))
                if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(
                tungstenite::Error::ConnectionClosed
                | tungstenite::Error::AlreadyClosed
                | tungstenite::Error::Protocol(
                    tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
                ),
            ) => return Ok(()),
            Err(error) => return Err(format!("read contract printer-event stream: {error}")),
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn printer_snapshot_frames() -> [String; 3] {
    let response = serde_json::from_str::<serde_json::Value>(printer_response())
        .expect("contract printer response is valid JSON");
    let printer = &response["devices"][0];
    [
        r#"{"type":"snapshot_begin","version":1}"#.to_owned(),
        format!(r#"{{"type":"printer_upsert","printer":{printer}}}"#),
        r#"{"type":"snapshot_end","version":1}"#.to_owned(),
    ]
}

fn read_request(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("configure contract print request timeout: {error}"))?;
    let mut request = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    let header_end = loop {
        read_more(stream, &mut request, &mut buffer)?;
        if let Some(position) = find_bytes(&request, b"\r\n\r\n") {
            break position + 4;
        }
    };
    let headers = String::from_utf8_lossy(&request[..header_end]).to_ascii_lowercase();
    if headers.contains("expect: 100-continue") {
        stream
            .write_all(b"HTTP/1.1 100 Continue\r\n\r\n")
            .map_err(|error| format!("send contract continue response: {error}"))?;
    }
    if let Some(length) = content_length(&headers)? {
        while request.len() < header_end + length {
            read_more(stream, &mut request, &mut buffer)?;
        }
    } else if headers.contains("transfer-encoding: chunked") {
        while !request[header_end..].ends_with(b"0\r\n\r\n") {
            read_more(stream, &mut request, &mut buffer)?;
        }
    }
    Ok(request)
}

fn read_more(
    stream: &mut TcpStream,
    request: &mut Vec<u8>,
    buffer: &mut [u8],
) -> Result<(), String> {
    let count = stream
        .read(buffer)
        .map_err(|error| format!("read contract print request: {error}"))?;
    if count == 0 {
        return Err("contract print request ended before its body completed".to_owned());
    }
    if request.len().saturating_add(count) > MAX_REQUEST_BYTES {
        return Err("contract print request exceeded 2 MiB".to_owned());
    }
    request.extend_from_slice(&buffer[..count]);
    Ok(())
}

fn content_length(headers: &str) -> Result<Option<usize>, String> {
    let Some(line) = headers
        .lines()
        .find(|line| line.starts_with("content-length:"))
    else {
        return Ok(None);
    };
    line.split_once(':')
        .and_then(|(_, value)| value.trim().parse().ok())
        .map(Some)
        .ok_or_else(|| "contract print request has invalid content-length".to_owned())
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) -> Result<(), String> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|error| format!("write contract print response: {error}"))
}

fn printer_response() -> &'static str {
    r#"{"message":"success","devices":[{"dev_id":"contract-device","dev_name":"Contract Printer","name":"Contract Printer","dev_model_name":"N6","model":"N6","dev_online":true,"online":true,"task_status":"IDLE","state":"IDLE","gcode_state":"IDLE","mc_percent":0,"mc_remaining_time":0,"layer_num":0,"total_layer_num":0,"task_id":"","subtask_id":"","gcode_file":"","subtask_name":"","hms":[],"pandar_printer_id":"contract-printer","nozzle_temperatures":[],"active_nozzle":null,"bed_temperature_celsius":null,"bed_target_temperature_celsius":null,"chamber_temperature_celsius":null,"chamber_light_on":null,"materials":null}]}"#
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
