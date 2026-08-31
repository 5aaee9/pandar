use std::{
    ffi::c_void,
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use super::{PluginStudioModelTask, pandar_plugin_studio_get_model_task};
use crate::studio_print::ffi::StudioSnapshotCallback;
use crate::studio_print::{PluginBytes, PluginStudioAccount, PluginStudioSnapshot};

mod validation;

struct SnapshotState {
    hub_url: String,
    token: String,
    account_epoch: u64,
}

#[derive(Debug, PartialEq)]
struct CapturedTask {
    job_id: i32,
    design_id: i32,
    profile_id: i32,
    instance_id: i32,
    task_id: String,
    model_id: String,
    model_name: String,
    profile_name: String,
}

extern "C" fn current_snapshot(context: *mut c_void, snapshot: *mut PluginStudioSnapshot) -> i32 {
    let state = unsafe { &*context.cast::<SnapshotState>() };
    unsafe {
        *snapshot = snapshot_for(state);
    }
    1
}

extern "C" fn stale_snapshot(context: *mut c_void, snapshot: *mut PluginStudioSnapshot) -> i32 {
    current_snapshot(context, snapshot);
    unsafe {
        (*snapshot).account_epoch += 1;
    }
    1
}

extern "C" fn capture_task(context: *mut c_void, task: *const PluginStudioModelTask) -> i32 {
    let task = unsafe { &*task };
    let captured = CapturedTask {
        job_id: task.job_id,
        design_id: task.design_id,
        profile_id: task.profile_id,
        instance_id: task.instance_id,
        task_id: read(task.task_id),
        model_id: read(task.model_id),
        model_name: read(task.model_name),
        profile_name: read(task.profile_name),
    };
    unsafe {
        *context.cast::<Option<CapturedTask>>() = Some(captured);
    }
    1
}

#[test]
fn ordinary_model_task_is_delivered_through_the_ffi_boundary() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind model-task server");
    let hub_url = format!("http://{}", listener.local_addr().expect("server address"));
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept model-task request");
        let request = read_request(&mut stream);
        let body = r#"{"job_id":41,"design_id":0,"profile_id":0,"instance_id":0,"task_id":"41","model_id":"","model_name":" Project Alpha ","profile_name":" 0.20mm Standard "}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body,
        )
        .expect("write model-task response");
        request
    });

    let mut state = SnapshotState {
        hub_url,
        token: "task-token".to_owned(),
        account_epoch: 7,
    };
    let account = PluginStudioAccount {
        snapshot: snapshot_for(&state),
        context: (&mut state as *mut SnapshotState).cast(),
        current_snapshot: Some(current_snapshot as StudioSnapshotCallback),
    };
    let mut captured = None;
    let result = unsafe {
        pandar_plugin_studio_get_model_task(
            &account,
            bytes("41"),
            (&mut captured as *mut Option<CapturedTask>).cast(),
            Some(capture_task),
        )
    };

    assert_eq!(result.status, 0);
    assert_eq!(result.http_code, 200);
    assert_eq!(
        captured,
        Some(CapturedTask {
            job_id: 41,
            design_id: 0,
            profile_id: 0,
            instance_id: 0,
            task_id: "41".to_owned(),
            model_id: String::new(),
            model_name: " Project Alpha ".to_owned(),
            profile_name: " 0.20mm Standard ".to_owned(),
        })
    );
    unsafe {
        crate::pandar_plugin_free_with_capacity(
            result.body_ptr.cast(),
            result.body_len,
            result.body_cap,
        )
    };
    let request = server.join().expect("model-task server joined");
    assert!(request.starts_with("GET /api/v1/plugin/jobs/41/model-task HTTP/1.1"));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer task-token")
    );
}

#[test]
fn stale_account_response_is_rejected_without_delivery() {
    let body = r#"{"job_id":41,"design_id":0,"profile_id":0,"instance_id":0,"task_id":"41","model_id":"","model_name":"Project","profile_name":"Preset"}"#;
    let (hub_url, server) = model_task_server(body);
    let mut state = SnapshotState {
        hub_url,
        token: "task-token".to_owned(),
        account_epoch: 7,
    };
    let account = PluginStudioAccount {
        snapshot: snapshot_for(&state),
        context: (&mut state as *mut SnapshotState).cast(),
        current_snapshot: Some(stale_snapshot),
    };
    let mut captured = None;
    let result = unsafe {
        pandar_plugin_studio_get_model_task(
            &account,
            bytes("41"),
            (&mut captured as *mut Option<CapturedTask>).cast(),
            Some(capture_task),
        )
    };

    assert_eq!(result.status, 1);
    assert_eq!(result.http_code, 409);
    assert_eq!(captured, None);
    free(result);
    server.join().expect("stale response server joined");
}

#[test]
fn metadata_unavailable_error_is_preserved_without_delivery() {
    let body = r#"{"error":"studio_model_task_metadata_unavailable"}"#;
    let (hub_url, server) = model_task_server_with_status("409 Conflict", body);
    let mut state = SnapshotState {
        hub_url,
        token: "task-token".to_owned(),
        account_epoch: 7,
    };
    let account = PluginStudioAccount {
        snapshot: snapshot_for(&state),
        context: (&mut state as *mut SnapshotState).cast(),
        current_snapshot: Some(current_snapshot),
    };
    let mut captured = None;
    let result = unsafe {
        pandar_plugin_studio_get_model_task(
            &account,
            bytes("41"),
            (&mut captured as *mut Option<CapturedTask>).cast(),
            Some(capture_task),
        )
    };

    assert_eq!(result.status, 1);
    assert_eq!(result.http_code, 409);
    assert_eq!(captured, None);
    let result_body = unsafe { std::slice::from_raw_parts(result.body_ptr, result.body_len) };
    assert_eq!(result_body, body.as_bytes());
    free(result);
    server.join().expect("metadata response server joined");
}

fn snapshot_for(state: &SnapshotState) -> PluginStudioSnapshot {
    PluginStudioSnapshot {
        hub_url: bytes(&state.hub_url),
        token: bytes(&state.token),
        printer_id: bytes(""),
        printer_authorized: 0,
        account_transition_pending: 0,
        account_epoch: state.account_epoch,
        cache_generation: 0,
        firmware_generation: 0,
    }
}

fn bytes(value: &str) -> PluginBytes {
    PluginBytes {
        ptr: value.as_ptr(),
        len: value.len(),
    }
}

fn read(value: PluginBytes) -> String {
    unsafe { value.read("model_task") }.unwrap_or_else(|_| panic!("valid model-task text"))
}

fn read_request(stream: &mut std::net::TcpStream) -> String {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut chunk).expect("read model-task request");
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);
    }
    String::from_utf8(request).expect("model-task request is UTF-8")
}

fn model_task_server(body: &'static str) -> (String, thread::JoinHandle<String>) {
    model_task_server_with_status("200 OK", body)
}

fn model_task_server_with_status(
    status: &'static str,
    body: &'static str,
) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind model-task server");
    let hub_url = format!("http://{}", listener.local_addr().expect("server address"));
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept model-task request");
        let request = read_request(&mut stream);
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body,
        )
        .expect("write model-task response");
        request
    });
    (hub_url, server)
}

fn free(result: crate::PluginHttpResult) {
    unsafe {
        crate::pandar_plugin_free_with_capacity(
            result.body_ptr.cast(),
            result.body_len,
            result.body_cap,
        )
    };
}
