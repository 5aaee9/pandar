use std::{ffi::c_void, io::Write, net::TcpListener, thread};

use pandar_network_plugin::{
    PluginStudioDeliveryResult, pandar_plugin_studio_account_request_admitted,
    pandar_plugin_studio_add_subscription, pandar_plugin_studio_begin_account_transition,
    pandar_plugin_studio_complete_delivery, pandar_plugin_studio_connect_local,
    pandar_plugin_studio_del_subscription, pandar_plugin_studio_finish_account_transition,
    pandar_plugin_studio_heartbeat_plan, pandar_plugin_studio_local_generation,
    pandar_plugin_studio_prepare_connected, pandar_plugin_studio_prepare_message,
    pandar_plugin_studio_request_snapshot, pandar_plugin_studio_selected,
    pandar_plugin_studio_set_listener, pandar_plugin_studio_set_selected,
    pandar_plugin_studio_status_target_available, pandar_plugin_studio_take_work,
};

use super::{INITIAL_PRINTERS_RESPONSE, body, create_session, read_http_request};

#[path = "studio_session/selected_target.rs"]
mod selected_target;

const CLOUD_MESSAGE_LISTENER: i32 = 1;
const LOCAL_MESSAGE_LISTENER: i32 = 2;
const PRINTER_CONNECTED_LISTENER: i32 = 3;
const LOCAL_CONNECTED_LISTENER: i32 = 4;
const CLOUD_TUNNEL: i32 = 0;
const LOCAL_TUNNEL: i32 = 1;
const CLOUD_OFFLINE_WORK: i32 = 1;
const LOCAL_OFFLINE_WORK: i32 = 2;
const LOCAL_LOST_WORK: i32 = 3;

#[derive(Debug, Default, PartialEq, Eq)]
struct Payload {
    dev_id: String,
    body: String,
    printer_id: String,
    model: String,
}

#[derive(Debug, PartialEq, Eq)]
struct Target {
    tunnel: i32,
    dev_id: String,
    generation: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct Work {
    kind: i32,
    state: i32,
    ticket: u64,
    generation: u64,
    dev_id: String,
    body: String,
}

fn copied(ptr: *const u8, len: usize) -> String {
    String::from_utf8(unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec()).unwrap()
}

extern "C" fn copy_payload(
    context: *mut c_void,
    dev_id: *const u8,
    dev_id_len: usize,
    payload: *const u8,
    payload_len: usize,
    printer_id: *const u8,
    printer_id_len: usize,
    model: *const u8,
    model_len: usize,
) {
    let output = unsafe { &mut *context.cast::<Payload>() };
    output.dev_id = copied(dev_id, dev_id_len);
    output.body = copied(payload, payload_len);
    output.printer_id = copied(printer_id, printer_id_len);
    output.model = copied(model, model_len);
}

extern "C" fn copy_target(
    context: *mut c_void,
    tunnel: i32,
    dev_id: *const u8,
    dev_id_len: usize,
    generation: u64,
) {
    unsafe { &mut *context.cast::<Vec<Target>>() }.push(Target {
        tunnel,
        dev_id: copied(dev_id, dev_id_len),
        generation,
    });
}

extern "C" fn copy_work(
    context: *mut c_void,
    kind: i32,
    state: i32,
    ticket: u64,
    generation: u64,
    dev_id: *const u8,
    dev_id_len: usize,
    payload: *const u8,
    payload_len: usize,
) {
    unsafe { &mut *context.cast::<Vec<Work>>() }.push(Work {
        kind,
        state,
        ticket,
        generation,
        dev_id: copied(dev_id, dev_id_len),
        body: copied(payload, payload_len),
    });
}

fn set_listener(session: *mut c_void, listener: i32) {
    assert_eq!(
        pandar_plugin_studio_set_listener(session, listener, true),
        0
    );
}

fn prepare_connected(
    session: *mut c_void,
    now_ms: u64,
    output: &mut Payload,
) -> PluginStudioDeliveryResult {
    pandar_plugin_studio_prepare_connected(
        session,
        b"serial-1".as_ptr(),
        8,
        now_ms,
        (output as *mut Payload).cast(),
        Some(copy_payload),
    )
}

#[test]
fn selected_and_subscription_operations_are_local_atomic_and_total() {
    let session = create_session("http://127.0.0.1:9", "token");
    assert_eq!(body(pandar_plugin_studio_selected(session)), "");
    assert_eq!(
        pandar_plugin_studio_set_selected(session, b"serial-1".as_ptr(), 8),
        0
    );
    assert_eq!(body(pandar_plugin_studio_selected(session)), "serial-1");

    set_listener(session, CLOUD_MESSAGE_LISTENER);
    assert_eq!(
        pandar_plugin_studio_add_subscription(session, b"serial-1".as_ptr(), 8),
        0
    );
    let mut targets = Vec::new();
    let plan = pandar_plugin_studio_heartbeat_plan(
        session,
        (&mut targets as *mut Vec<Target>).cast(),
        Some(copy_target),
    );
    assert_eq!((plan.wait_ms, plan.refresh), (2_000, 1));
    assert_eq!(
        targets,
        vec![Target {
            tunnel: CLOUD_TUNNEL,
            dev_id: "serial-1".to_owned(),
            generation: 0,
        }]
    );

    assert_eq!(pandar_plugin_studio_begin_account_transition(session), 0);
    let transition = pandar_plugin_studio_request_snapshot(
        session,
        b"during-login".as_ptr(),
        12,
        std::ptr::null_mut(),
        None,
    );
    assert_eq!(body(pandar_plugin_studio_selected(session)), "");
    assert_eq!(
        pandar_plugin_studio_set_selected(session, b"during-login".as_ptr(), 12),
        0
    );
    assert_eq!(
        pandar_plugin_studio_add_subscription(session, b"during-login".as_ptr(), 12),
        0
    );
    assert_eq!(
        pandar_plugin_studio_finish_account_transition(session, transition.account_epoch),
        0
    );
    assert_eq!(body(pandar_plugin_studio_selected(session)), "during-login");
    super::pandar_plugin_printer_refresh_session_destroy(session);
}

#[test]
fn stale_account_transition_finish_cannot_release_a_newer_fence() {
    let session = create_session("http://127.0.0.1:9", "token");
    assert_eq!(pandar_plugin_studio_begin_account_transition(session), 0);
    let first = pandar_plugin_studio_request_snapshot(
        session,
        b"serial-1".as_ptr(),
        8,
        std::ptr::null_mut(),
        None,
    );
    assert_eq!(pandar_plugin_studio_begin_account_transition(session), 0);
    let second = pandar_plugin_studio_request_snapshot(
        session,
        b"serial-1".as_ptr(),
        8,
        std::ptr::null_mut(),
        None,
    );

    assert_eq!(
        pandar_plugin_studio_finish_account_transition(session, first.account_epoch),
        0
    );
    assert_eq!(pandar_plugin_studio_account_request_admitted(session), 0);
    assert_eq!(
        pandar_plugin_studio_finish_account_transition(session, second.account_epoch),
        0
    );
    assert_eq!(pandar_plugin_studio_account_request_admitted(session), 1);
    super::pandar_plugin_printer_refresh_session_destroy(session);
}

#[test]
fn connected_local_and_offline_work_use_once_only_rust_tickets() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub_url = format!("http://{}", listener.local_addr().unwrap());
    let online = INITIAL_PRINTERS_RESPONSE.replace(
        r#""dev_online":false,"online":false"#,
        r#""dev_online":true,"online":true"#,
    );
    let offline = INITIAL_PRINTERS_RESPONSE.to_owned();
    let server = thread::spawn(move || {
        for response in [online, offline] {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_http_request(&mut stream);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(),
                response
            )
            .unwrap();
        }
    });
    let session = create_session(&hub_url, "token");
    for listener in [
        CLOUD_MESSAGE_LISTENER,
        LOCAL_MESSAGE_LISTENER,
        PRINTER_CONNECTED_LISTENER,
        LOCAL_CONNECTED_LISTENER,
    ] {
        set_listener(session, listener);
    }
    assert_eq!(
        pandar_plugin_studio_set_selected(session, b"serial-1".as_ptr(), 8),
        0
    );
    assert_eq!(super::refresh_without_observation(session).status, 0);

    let mut connected_payload = Payload::default();
    let connected = prepare_connected(session, 100, &mut connected_payload);
    assert_eq!(connected.status, 0);
    assert_eq!(connected_payload.body, "tunnel/serial-1");
    assert_eq!(
        pandar_plugin_studio_complete_delivery(session, connected.ticket, true),
        1
    );
    assert_ne!(
        prepare_connected(session, 500, &mut Payload::default()).status,
        0
    );
    let retry = prepare_connected(session, 1_100, &mut Payload::default());
    assert_eq!(retry.status, 0);
    assert_eq!(
        pandar_plugin_studio_complete_delivery(session, retry.ticket, false),
        0
    );

    let mut local_payload = Payload::default();
    let local = pandar_plugin_studio_connect_local(
        session,
        b"serial-1".as_ptr(),
        8,
        (&mut local_payload as *mut Payload).cast(),
        Some(copy_payload),
    );
    assert_eq!(local.status, 0);
    assert!(local_payload.body.contains(r#""dev_id":"serial-1""#));
    assert_eq!(
        pandar_plugin_studio_complete_delivery(session, local.ticket, true),
        1
    );
    assert_eq!(
        pandar_plugin_studio_local_generation(session, b"serial-1".as_ptr(), 8),
        local.local_generation
    );

    let mut status_payload = Payload::default();
    let message = pandar_plugin_studio_prepare_message(
        session,
        CLOUD_TUNNEL,
        b"serial-1".as_ptr(),
        8,
        0,
        false,
        0,
        (&mut status_payload as *mut Payload).cast(),
        Some(copy_payload),
    );
    assert_eq!(message.status, 0);
    assert!(status_payload.body.contains(r#""command":"push_status""#));
    assert_eq!(
        pandar_plugin_studio_complete_delivery(session, message.ticket, true),
        1
    );
    assert_eq!(
        pandar_plugin_studio_complete_delivery(session, message.ticket, true),
        0
    );

    let offline_result = super::refresh_without_observation(session);
    assert_eq!(offline_result.status, 0);
    let _ = body(offline_result);
    let mut work: Vec<Work> = Vec::new();
    assert_eq!(
        pandar_plugin_studio_take_work(
            session,
            (&mut work as *mut Vec<Work>).cast(),
            Some(copy_work),
        ),
        0
    );
    assert_eq!(
        work.iter().map(|item| item.kind).collect::<Vec<_>>(),
        vec![CLOUD_OFFLINE_WORK, LOCAL_OFFLINE_WORK, LOCAL_LOST_WORK]
    );
    assert!(work.iter().all(|item| {
        item.dev_id == "serial-1" && item.body == r#"{"event":{"event":"client.disconnected"}}"#
    }));
    assert_eq!(
        pandar_plugin_studio_set_selected(session, b"serial-2".as_ptr(), 8),
        0
    );
    for item in &work {
        assert_eq!(
            pandar_plugin_studio_complete_delivery(session, item.ticket, true),
            i32::from(item.kind != CLOUD_OFFLINE_WORK)
        );
        assert_eq!(
            pandar_plugin_studio_complete_delivery(session, item.ticket, true),
            0
        );
    }
    assert_eq!(
        pandar_plugin_studio_local_generation(session, b"serial-1".as_ptr(), 8),
        0
    );
    super::pandar_plugin_printer_refresh_session_destroy(session);
    server.join().unwrap();
}
