use std::{
    ffi::c_void,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use pandar_network_plugin::{
    PluginHttpResult, pandar_plugin_free_with_capacity, pandar_plugin_printer_refresh,
    pandar_plugin_printer_refresh_session_create, pandar_plugin_printer_refresh_session_destroy,
    pandar_plugin_printer_refresh_session_update,
};

const INITIAL_PRINTERS_RESPONSE: &str = r#"{"message":"success","devices":[{"dev_id":"serial-1","dev_name":"Printer","name":"Printer","dev_ip":null,"dev_access_code":null,"dev_model_name":null,"model":null,"dev_online":false,"online":false,"task_status":"unknown","state":"unknown","gcode_state":null,"mc_percent":null,"mc_remaining_time":null,"layer_num":null,"total_layer_num":null,"task_id":null,"subtask_id":null,"gcode_file":null,"subtask_name":null,"hms":[],"pandar_printer_id":"printer-1","nozzle_temperatures":[],"active_nozzle":null,"bed_temperature_celsius":null,"bed_target_temperature_celsius":null,"chamber_temperature_celsius":null,"chamber_light_on":null,"materials":null}]}"#;

#[derive(Debug, PartialEq, Eq)]
enum RefreshEvent {
    Reserved(u64),
    Requested(u64),
}

struct ReservationContext {
    next: Arc<AtomicU64>,
    assigned: Arc<AtomicU64>,
    events: mpsc::Sender<RefreshEvent>,
}

extern "C" fn reserve_observation(context: *mut c_void) {
    let context = unsafe { &*context.cast::<ReservationContext>() };
    let sequence = context.next.fetch_add(1, Ordering::SeqCst) + 1;
    context.assigned.store(sequence, Ordering::SeqCst);
    let _ = context.events.send(RefreshEvent::Reserved(sequence));
}

fn read_http_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0);
        request.extend_from_slice(&buffer[..read]);
    }
    String::from_utf8(request).unwrap()
}

fn body(result: PluginHttpResult) -> String {
    if result.body_ptr.is_null() || result.body_len == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(result.body_ptr, result.body_len) };
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap);
    body
}

fn create_session(hub_url: &str, token: &str) -> *mut c_void {
    pandar_plugin_printer_refresh_session_create(
        hub_url.as_ptr(),
        hub_url.len(),
        token.as_ptr(),
        token.len(),
    )
}

fn refresh_without_observation(session: *mut c_void) -> PluginHttpResult {
    pandar_plugin_printer_refresh(session, std::ptr::null_mut(), None)
}

#[test]
fn admitted_refresh_reserves_in_request_order_and_busy_refresh_reserves_nothing() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub_url = format!("http://{}", listener.local_addr().unwrap());
    let (events_tx, events_rx) = mpsc::channel();
    let server_events = events_tx.clone();
    let (first_started_tx, first_started_rx) = mpsc::channel();
    let (release_first_tx, release_first_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        for request_index in 1..=2 {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            assert!(request.starts_with("GET /api/v1/plugin/printers HTTP/1.1\r\n"));
            server_events
                .send(RefreshEvent::Requested(request_index))
                .unwrap();
            if request_index == 1 {
                first_started_tx.send(()).unwrap();
                release_first_rx.recv().unwrap();
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{INITIAL_PRINTERS_RESPONSE}",
                INITIAL_PRINTERS_RESPONSE.len()
            )
            .unwrap();
        }
    });
    let session = create_session(&hub_url, "token");
    let sequence = Arc::new(AtomicU64::new(0));
    let first_assigned = Arc::new(AtomicU64::new(0));
    let first_context = ReservationContext {
        next: Arc::clone(&sequence),
        assigned: Arc::clone(&first_assigned),
        events: events_tx.clone(),
    };
    let session_address = session as usize;
    let first = thread::spawn(move || {
        let mut context = first_context;
        let result = pandar_plugin_printer_refresh(
            session_address as *mut c_void,
            (&mut context as *mut ReservationContext).cast(),
            Some(reserve_observation),
        );
        (result.status, body(result))
    });
    first_started_rx.recv().unwrap();

    let busy_assigned = Arc::new(AtomicU64::new(0));
    let mut busy_context = ReservationContext {
        next: Arc::clone(&sequence),
        assigned: Arc::clone(&busy_assigned),
        events: events_tx.clone(),
    };
    let busy = pandar_plugin_printer_refresh(
        session,
        (&mut busy_context as *mut ReservationContext).cast(),
        Some(reserve_observation),
    );
    assert_ne!(busy.status, 0);
    assert_eq!(body(busy), r#"{"error":"hub_unavailable"}"#);
    assert_eq!(busy_assigned.load(Ordering::SeqCst), 0);

    release_first_tx.send(()).unwrap();
    let (first_status, first_body) = first.join().unwrap();
    assert_eq!(first_status, 0);
    assert_eq!(first_body, INITIAL_PRINTERS_RESPONSE);

    let second_assigned = Arc::new(AtomicU64::new(0));
    let mut second_context = ReservationContext {
        next: Arc::clone(&sequence),
        assigned: Arc::clone(&second_assigned),
        events: events_tx,
    };
    let second = pandar_plugin_printer_refresh(
        session,
        (&mut second_context as *mut ReservationContext).cast(),
        Some(reserve_observation),
    );
    assert_eq!(second.status, 0);
    assert_eq!(body(second), INITIAL_PRINTERS_RESPONSE);
    server.join().unwrap();

    assert_eq!(first_assigned.load(Ordering::SeqCst), 1);
    assert_eq!(second_assigned.load(Ordering::SeqCst), 2);
    assert_eq!(sequence.load(Ordering::SeqCst), 2);
    assert_eq!(
        (0..4)
            .map(|_| events_rx.recv_timeout(Duration::from_secs(1)).unwrap())
            .collect::<Vec<_>>(),
        vec![
            RefreshEvent::Reserved(1),
            RefreshEvent::Requested(1),
            RefreshEvent::Reserved(2),
            RefreshEvent::Requested(2),
        ]
    );
    pandar_plugin_printer_refresh_session_destroy(session);
}

#[test]
fn status_refresh_rejects_invalid_success_body() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub_url = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        assert!(request.starts_with("GET /api/v1/plugin/printers HTTP/1.1\r\n"));
        let body = r#"{"message":"success","devices":["#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let session = create_session(&hub_url, "token");

    let result = refresh_without_observation(session);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 200);
    assert_eq!(body(result), r#"{"error":"invalid_response"}"#);
    pandar_plugin_printer_refresh_session_destroy(session);
    server.join().unwrap();
}

#[test]
fn status_refresh_accepts_nullable_initial_printer_status() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub_url = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        assert!(request.starts_with("GET /api/v1/plugin/printers HTTP/1.1\r\n"));
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{INITIAL_PRINTERS_RESPONSE}",
            INITIAL_PRINTERS_RESPONSE.len()
        )
        .unwrap();
    });
    let session = create_session(&hub_url, "token");

    let result = refresh_without_observation(session);

    assert_eq!(result.status, 0);
    assert_eq!(result.http_code, 200);
    assert_eq!(body(result), INITIAL_PRINTERS_RESPONSE);
    pandar_plugin_printer_refresh_session_destroy(session);
    server.join().unwrap();
}

#[test]
fn status_refresh_discards_response_after_credentials_change() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub_url = format!("http://{}", listener.local_addr().unwrap());
    let (request_started_tx, request_started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        assert!(request.starts_with("GET /api/v1/plugin/printers HTTP/1.1\r\n"));
        request_started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{INITIAL_PRINTERS_RESPONSE}",
            INITIAL_PRINTERS_RESPONSE.len()
        )
        .unwrap();
    });
    let session = create_session(&hub_url, "old-token");
    let session_address = session as usize;
    let refresh = thread::spawn(move || {
        let result = refresh_without_observation(session_address as *mut c_void);
        (result.status, result.http_code, body(result))
    });
    request_started_rx.recv().unwrap();

    assert_eq!(
        pandar_plugin_printer_refresh_session_update(
            session,
            hub_url.as_ptr(),
            hub_url.len(),
            b"new-token".as_ptr(),
            b"new-token".len(),
        ),
        0
    );
    release_tx.send(()).unwrap();
    let (status, http_code, response_body) = refresh.join().unwrap();

    assert_ne!(status, 0);
    assert_eq!(http_code, 0);
    assert_eq!(response_body, r#"{"error":"hub_unavailable"}"#);
    pandar_plugin_printer_refresh_session_destroy(session);
    server.join().unwrap();
}

#[test]
fn concurrent_status_refresh_returns_without_waiting_for_in_flight_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub_url = format!("http://{}", listener.local_addr().unwrap());
    let (request_started_tx, request_started_rx) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        assert!(request.starts_with("GET /api/v1/plugin/printers HTTP/1.1\r\n"));
        request_started_tx.send(()).unwrap();
        thread::sleep(Duration::from_secs(1));
    });
    let session = create_session(&hub_url, "token");
    let session_address = session as usize;
    let in_flight = thread::spawn(move || {
        let result = refresh_without_observation(session_address as *mut c_void);
        (result.status, body(result))
    });
    request_started_rx.recv().unwrap();
    let started = Instant::now();

    let concurrent = refresh_without_observation(session);
    let elapsed = started.elapsed();

    assert_ne!(concurrent.status, 0);
    assert_eq!(body(concurrent), r#"{"error":"hub_unavailable"}"#);
    let (first_status, first_body) = in_flight.join().unwrap();
    assert_ne!(first_status, 0);
    assert_eq!(first_body, r#"{"error":"hub_unavailable"}"#);
    pandar_plugin_printer_refresh_session_destroy(session);
    server.join().unwrap();
    assert!(
        elapsed < Duration::from_millis(250),
        "concurrent status refresh waited behind the in-flight request: {elapsed:?}"
    );
}

#[test]
fn status_refresh_times_out_without_waiting_for_hung_hub() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub_url = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        assert!(request.starts_with("GET /api/v1/plugin/printers HTTP/1.1\r\n"));
        thread::sleep(Duration::from_secs(3));
    });
    let session = create_session(&hub_url, "token");
    let started = Instant::now();

    let result = refresh_without_observation(session);

    assert!(
        started.elapsed() < Duration::from_secs(2),
        "status refresh exceeded its wall-clock bound: {:?}",
        started.elapsed()
    );
    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 0);
    assert_eq!(body(result), r#"{"error":"hub_unavailable"}"#);
    pandar_plugin_printer_refresh_session_destroy(session);
    server.join().unwrap();
}
