use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use crate::{harness::DIAGNOSTIC_SECRET, support::read_http_request_with_timeout};

pub(super) struct MockHub {
    pub(super) url: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    handle: thread::JoinHandle<()>,
}

impl MockHub {
    pub(super) fn spawn(case: &str, race_directory: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let thread_requests = Arc::clone(&requests);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let job_polls = Arc::new(AtomicUsize::new(0));
        let case = case.to_owned();
        let race_directory = race_directory.to_owned();
        let handle = thread::spawn(move || {
            while !thread_stop.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("mock Hub accept failed: {error}"),
                };
                stream.set_nonblocking(false).unwrap();
                let cancelled_upload =
                    case == "cancel_upload" && !thread_requests.lock().unwrap().is_empty();
                let request = if cancelled_upload {
                    read_cancelled_upload(&mut stream)
                } else {
                    read_http_request_with_timeout(&mut stream, Some(Duration::from_secs(2)))
                };
                thread_requests.lock().unwrap().push(request.clone());
                if cancelled_upload {
                    continue;
                }
                if is_printer_events_upgrade(&request) {
                    thread::spawn(move || serve_printer_events(stream, &request));
                    continue;
                }
                let first = request.lines().next().unwrap_or_default();
                hold_task_response(first, &case, &race_directory);
                let (status, body) = response_for(first, &job_polls, &case);
                if let Err(error) = write_response(&mut stream, status, &body) {
                    assert!(
                        matches!(
                            case.as_str(),
                            "model_task_destroy_inflight" | "model_task_destroy_no_auth_recovery"
                        ),
                        "mock Hub response failed: {error}"
                    );
                }
            }
        });
        Self {
            url,
            requests,
            stop,
            handle,
        }
    }

    pub(super) fn finish(self) -> Vec<String> {
        self.stop.store(true, Ordering::SeqCst);
        self.handle.join().expect("mock Hub thread");
        Arc::try_unwrap(self.requests)
            .expect("mock Hub request ownership")
            .into_inner()
            .unwrap()
    }
}

fn hold_task_response(first: &str, case: &str, race_directory: &Path) {
    if case == "model_task_destroy_no_auth_recovery"
        && first.starts_with("POST /api/v1/plugin/no-auth-session ")
    {
        std::fs::create_dir(race_directory.join("request-entered")).unwrap();
        thread::sleep(Duration::from_secs(3));
        return;
    }
    if case == "model_task_destroy_inflight"
        && first.starts_with("GET /api/v1/plugin/jobs/38191/model-task ")
    {
        std::fs::create_dir(race_directory.join("request-entered")).unwrap();
        thread::sleep(Duration::from_secs(3));
        return;
    }
    let should_hold = match case {
        "stale_task_list" => first.starts_with("GET /api/v1/plugin/jobs?"),
        "stale_task_plate" => first.starts_with("GET /api/v1/plugin/jobs/38191/plate "),
        "stale_task_subtask" => first.starts_with("GET /api/v1/plugin/jobs/38191/subtask "),
        "stale_model_task" => first.starts_with("GET /api/v1/plugin/jobs/38191/model-task "),
        "stale_during_detail" => first.starts_with("GET /api/v1/plugin/jobs/38191 "),
        _ => false,
    };
    if !should_hold {
        return;
    }
    std::fs::create_dir(race_directory.join("request-entered")).unwrap();
    let release = race_directory.join("release-request");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !release.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        release.exists(),
        "account freshness request was not released"
    );
}

fn read_cancelled_upload(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => request.extend_from_slice(&buffer[..read]),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                ) =>
            {
                break;
            }
            Err(error) => panic!("cancelled upload read failed: {error}"),
        }
    }
    String::from_utf8_lossy(&request).into_owned()
}

#[path = "mock_hub/responses.rs"]
mod responses;
use responses::response_for;
fn is_printer_events_upgrade(request: &str) -> bool {
    let request_line = request.lines().next().unwrap_or_default();
    let upgrade_header = request.lines().any(|line| {
        line.split_once(':')
            .is_some_and(|(name, _value)| name.eq_ignore_ascii_case("upgrade"))
    });
    request_line
        == "GET /api/v1/tenants/contract-tenant/printer-events?projection=studio&version=1 HTTP/1.1"
        && upgrade_header
}

/// Completes the printer-events WebSocket upgrade and serves a full snapshot
/// of the contract printer, then keeps the stream open until the peer goes
/// away or the probe process exits.
fn serve_printer_events(stream: TcpStream, request: &str) {
    let key = request.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("sec-websocket-key")
            .then(|| value.trim().to_owned())
    });
    let Some(key) = key else {
        eprintln!("contract mock hub stream upgrade lacked a WebSocket key");
        return;
    };
    let mut stream = stream;
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
    let mut snapshot_pending = true;
    loop {
        if snapshot_pending {
            for frame in crate::harness::snapshot_frames(printer_response()) {
                if ws.write(tungstenite::Message::text(frame)).is_err() {
                    return;
                }
            }
            let _ = ws.flush();
            snapshot_pending = false;
        }
        match ws.read() {
            Ok(_message) => {
                // Inbound ping/bookkeeping frames; tungstenite queues pongs.
                let _ = ws.flush();
            }
            Err(tungstenite::Error::Io(error))
                if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return,
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn write_response(stream: &mut impl Write, status: &str, body: &str) -> std::io::Result<()> {
    write!(
        stream,
        "{status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn printer_response() -> &'static str {
    r#"{"message":"success","devices":[{"dev_id":"studio-serial-1","dev_name":"Contract Printer","name":"Contract Printer","dev_ip":null,"dev_access_code":null,"dev_model_name":"N6","model":"N6","dev_online":true,"online":true,"task_status":"IDLE","state":"IDLE","gcode_state":"IDLE","mc_percent":0,"mc_remaining_time":0,"layer_num":0,"total_layer_num":0,"task_id":"","subtask_id":"","gcode_file":"","subtask_name":"","hms":[],"pandar_printer_id":"printer-1","nozzle_temperatures":[],"active_nozzle":null,"bed_temperature_celsius":null,"bed_target_temperature_celsius":null,"chamber_temperature_celsius":null,"chamber_light_on":null,"materials":null}]}"#
}

fn task_page_response() -> &'static str {
    r#"{"total":1,"hits":[{"id":38191,"status":1,"designId":0,"title":"contract-base.3mf","deviceName":"Contract Printer","deviceId":"studio-serial-1","cover":"","startTime":"2026-07-20T12:00:00Z","endTime":"","profileId":38191}]}"#
}

fn subtask_response() -> &'static str {
    r##"{"content":"{\"info\":{\"plate_idx\":7}}","context":{"plates":[{"index":7,"thumbnail":{"url":""},"prediction":3600,"weight":12.5,"filaments":[{"color":"#FFFFFFFF","type":"PLA","used_g":"12.5","used_m":"4.2"}]}]}}"##
}
