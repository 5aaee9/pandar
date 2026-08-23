//! Stream-equivalent coverage for the retired printer polling suite: a mock
//! Hub accepts the Studio-projection WebSocket upgrade, emits scripted
//! snapshot frames plus live follow-up frames, and the tests assert cache and
//! delivery behavior through the public FFI.

#![cfg(unix)]

use std::{
    ffi::c_void,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

use pandar_network_plugin::{
    PluginConnectionResult, PluginHttpResult, StudioHeartbeatVisitor, StudioWorkVisitor,
    pandar_plugin_connection_set_account_epoch, pandar_plugin_connection_take_offline,
    pandar_plugin_connection_take_transition, pandar_plugin_connection_visit_printers,
    pandar_plugin_free_with_capacity, pandar_plugin_printer_refresh,
    pandar_plugin_printer_refresh_session_create, pandar_plugin_printer_refresh_session_destroy,
    pandar_plugin_printer_refresh_session_set_tenant, pandar_plugin_studio_heartbeat_plan,
    pandar_plugin_studio_set_listener, pandar_plugin_studio_set_selected,
    pandar_plugin_studio_take_work,
};
use tungstenite::Message;

const DEVICE_A_ONLINE: &str = r#"{"dev_id":"serial-1","dev_name":"Printer A","name":"Printer A","dev_model_name":"N1","model":"N1","dev_online":true,"online":true,"task_status":"IDLE","state":"IDLE","gcode_state":"IDLE","mc_percent":7,"hms":[],"pandar_printer_id":"printer-1","nozzle_temperatures":[],"active_nozzle":null}"#;
const DEVICE_B_ONLINE: &str = r#"{"dev_id":"serial-2","dev_name":"Printer B","name":"Printer B","dev_model_name":null,"model":null,"dev_online":true,"online":true,"task_status":"IDLE","state":"IDLE","hms":[],"pandar_printer_id":"printer-2","nozzle_temperatures":[],"active_nozzle":null}"#;

/// One accepted hub connection: its upgrade request plus a channel the test
/// uses to stream follow-up frames over the live WebSocket.
struct HubRequest {
    request_line: String,
    authorization: Option<String>,
    accepted_at: std::time::Instant,
    frames: mpsc::Sender<String>,
    pongs: Arc<AtomicUsize>,
}

/// A connection script: emit frames then optionally close, or reject the
/// upgrade with HTTP 401.
#[derive(Clone)]
enum Script {
    Frames(String),
    Reject401,
}

/// Accepts hub connections forever. Each connection consumes one queued
/// script; with nothing queued it upgrades empty (worker self-reconnects).
type HubHandle = (
    SocketAddr,
    Arc<Mutex<Vec<Arc<HubRequest>>>>,
    mpsc::Sender<Option<Script>>,
);

#[allow(clippy::type_complexity)]
fn spawn_hub() -> HubHandle {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let requests: Arc<Mutex<Vec<Arc<HubRequest>>>> = Arc::new(Mutex::new(Vec::new()));
    let (script_tx, script_rx) = mpsc::channel::<Option<Script>>();
    let requests_for_server = Arc::clone(&requests);
    std::thread::spawn(move || {
        let requests = requests_for_server;
        loop {
            let (stream, _) = match listener.accept() {
                Ok(accepted) => accepted,
                Err(_) => return,
            };
            let script = match script_rx.recv_timeout(Duration::from_millis(400)) {
                Ok(Some(script)) => Some(script),
                Ok(None) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            };
            let requests = Arc::clone(&requests);
            std::thread::spawn(move || serve_connection(stream, requests, script));
        }
    });
    (addr, requests, script_tx)
}

#[allow(clippy::result_large_err)]
fn serve_connection(
    stream: TcpStream,
    requests: Arc<Mutex<Vec<Arc<HubRequest>>>>,
    script: Option<Script>,
) {
    let reject = matches!(script, Some(Script::Reject401));
    let (frame_tx, frame_rx) = mpsc::channel::<String>();
    let recorded: Mutex<Option<Arc<HubRequest>>> = Mutex::new(None);
    type HandshakeResponse = tungstenite::handshake::server::Response;
    let ws = tungstenite::accept_hdr(
        stream,
        |request: &tungstenite::http::Request<()>, response: HandshakeResponse| {
            let request_line = request
                .uri()
                .path_and_query()
                .map(|value| value.as_str().to_owned())
                .unwrap_or_default();
            let authorization = request
                .headers()
                .get("Authorization")
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned);
            let hub_request = Arc::new(HubRequest {
                request_line,
                authorization,
                accepted_at: std::time::Instant::now(),
                frames: frame_tx.clone(),
                pongs: Arc::new(AtomicUsize::new(0)),
            });
            *recorded.lock().unwrap() = Some(Arc::clone(&hub_request));
            requests.lock().unwrap().push(hub_request);
            if reject {
                let error_response = tungstenite::http::Response::builder()
                    .status(tungstenite::http::StatusCode::UNAUTHORIZED)
                    .body(Some(String::new()))
                    .expect("static 401 response");
                return Err(error_response);
            }
            Ok(response)
        },
    );
    let mut ws = match ws {
        Ok(ws) => ws,
        Err(error) => {
            eprintln!("pandar mock hub upgrade rejected: {error}");
            return;
        }
    };

    let mut close_after_send = false;
    if let Some(Script::Frames(frames)) = script {
        for frame in frames.lines().filter(|line| !line.is_empty()) {
            if frame == "@close" {
                close_after_send = true;
                break;
            }
            if ws.write(Message::text(frame)).is_err() {
                return;
            }
        }
        let _ = ws.flush();
        if close_after_send {
            let _ = ws.get_ref().shutdown(std::net::Shutdown::Both);
            return;
        }
    }
    let _ = ws.get_ref().set_nonblocking(true);
    loop {
        while let Ok(frame) = frame_rx.try_recv() {
            let message = if frame == "@ping" {
                Message::Ping(vec![1, 2, 3].into())
            } else {
                Message::text(frame)
            };
            if ws.write(message).is_err() {
                return;
            }
            let _ = ws.flush();
        }
        if let Ok(message) = ws.read() {
            if matches!(message, Message::Pong(_))
                && let Some(request) = recorded.lock().unwrap().as_ref()
            {
                request.pongs.fetch_add(1, Ordering::Relaxed);
            }
            let _ = ws.flush();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

// ---- frame helpers ---------------------------------------------------------

fn snapshot_script(devices: &[&str]) -> String {
    let mut script = String::from("{\"type\":\"snapshot_begin\",\"version\":1}\n");
    for device in devices {
        script.push_str(&format!(
            "{{\"type\":\"printer_upsert\",\"printer\":{device}}}\n"
        ));
    }
    script.push_str("{\"type\":\"snapshot_end\"}\n");
    script
}

fn upsert_frame(device: &str) -> String {
    format!("{{\"type\":\"printer_upsert\",\"printer\":{device}}}")
}

fn removed_frame(dev_id: &str) -> String {
    format!(
        "{{\"type\":\"printer_removed\",\"dev_id\":\"{dev_id}\",\"pandar_printer_id\":\"printer-1\"}}"
    )
}

// ---- session helpers -------------------------------------------------------

fn create_session(hub_url: &str) -> *mut c_void {
    pandar_plugin_printer_refresh_session_create(
        hub_url.as_ptr(),
        hub_url.len(),
        b"stream-token".as_ptr(),
        "stream-token".len(),
    )
}

fn set_tenant(session: *mut c_void, tenant: &str) {
    assert_eq!(
        0,
        pandar_plugin_printer_refresh_session_set_tenant(session, tenant.as_ptr(), tenant.len())
    );
}

fn body(result: PluginHttpResult) -> (i32, u32, String) {
    unsafe {
        let bytes = std::slice::from_raw_parts(result.body_ptr, result.body_len);
        let text = String::from_utf8_lossy(bytes).to_string();
        pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap);
        (result.status, result.http_code, text)
    }
}

fn cached_print_info(session: *mut c_void) -> (i32, u32, String) {
    body(pandar_plugin_printer_refresh(
        session,
        std::ptr::null_mut(),
        None,
    ))
}

fn wait_until(predicate: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !predicate() {
        assert!(
            std::time::Instant::now() < deadline,
            "condition not reached in time"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

extern "C" fn collect_printer(
    context: *mut c_void,
    dev_id: *const u8,
    dev_id_len: usize,
    _: *const u8,
    _: usize,
    _: *const u8,
    _: usize,
    _: *const u8,
    _: usize,
    _: i32,
) {
    let printers = unsafe { &*context.cast::<Mutex<Vec<String>>>() };
    let dev_id = unsafe { std::slice::from_raw_parts(dev_id, dev_id_len) };
    printers
        .lock()
        .unwrap()
        .push(String::from_utf8_lossy(dev_id).to_string());
}

extern "C" fn collect_offline(context: *mut c_void, dev_id: *const u8, dev_id_len: usize, _: u64) {
    let offline = unsafe { &*context.cast::<Mutex<Vec<String>>>() };
    let dev_id = unsafe { std::slice::from_raw_parts(dev_id, dev_id_len) };
    offline
        .lock()
        .unwrap()
        .push(String::from_utf8_lossy(dev_id).to_string());
}

extern "C" fn collect_work(
    context: *mut c_void,
    kind: i32,
    _: i32,
    _: u64,
    _: u64,
    dev_id: *const u8,
    dev_id_len: usize,
    body_ptr: *const u8,
    body_len: usize,
) {
    let work = unsafe { &*context.cast::<Mutex<Vec<(i32, String, String)>>>() };
    let dev_id = unsafe { std::slice::from_raw_parts(dev_id, dev_id_len) };
    let body = unsafe { std::slice::from_raw_parts(body_ptr, body_len) };
    work.lock().unwrap().push((
        kind,
        String::from_utf8_lossy(dev_id).to_string(),
        String::from_utf8_lossy(body).to_string(),
    ));
}

extern "C" fn noop_heartbeat(_: *mut c_void, _: i32, _: *const u8, _: usize, _: u64) {}

// ---- tests -----------------------------------------------------------------

#[path = "printer_stream/caching.rs"]
mod caching;
#[path = "printer_stream/lifecycle.rs"]
mod lifecycle;
