mod support;

#[path = "http_boundary/firmware.rs"]
mod http_boundary_firmware;
#[path = "http_boundary/firmware_safety.rs"]
mod http_boundary_firmware_safety;
#[path = "http_boundary/general.rs"]
mod http_boundary_general;
#[path = "http_boundary/print_submission.rs"]
mod http_boundary_print_submission;
#[path = "http_boundary/printer_operations.rs"]
mod http_boundary_printer_operations;

use pandar_network_plugin::{
    PluginHttpResult, pandar_plugin_exchange_ticket, pandar_plugin_free_with_capacity,
    pandar_plugin_get_jobs, pandar_plugin_get_printers, pandar_plugin_submit_print,
    pandar_plugin_submit_printer_operation,
};
use std::{fs, io::Write, net::TcpListener, path::Path, thread};
use support::{
    assert_multipart_file_part, assert_multipart_print_request, read_http_request_with_timeout,
};

const TOKEN: &[u8] = b"pandar_plugin_test_token";

fn body(result: PluginHttpResult) -> String {
    if result.body_ptr.is_null() || result.body_len == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(result.body_ptr, result.body_len) };
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap);
    body
}

fn one_shot_server(
    expected_method: &'static str,
    expected_path: &'static str,
    expected_bearer: Option<&'static str>,
    status_line: &'static str,
    body: &'static str,
    inspect_request: Option<fn(&str)>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request_with_timeout(&mut stream, None);
        let mut lines = request.lines();
        assert_eq!(
            lines.next().unwrap(),
            format!("{expected_method} {expected_path} HTTP/1.1")
        );
        if let Some(token) = expected_bearer {
            assert!(
                request.contains(&format!("authorization: Bearer {token}")),
                "request did not contain expected bearer header: {request}"
            );
        }
        if let Some(inspect_request) = inspect_request {
            inspect_request(&request);
        }
        let response = format!(
            "{status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    url
}

fn assert_plugin_multipart_print_request(request: &str) {
    assert_multipart_print_request(request);
    assert!(
        !request.contains(r#"name="ams_mapping""#)
            && !request.contains(r#"name="ams_mapping2""#)
            && !request.contains("\r\nnull\r\n"),
        "empty mapping fields should be omitted: {request}"
    );
    assert_multipart_file_part(request, "job.3mf", b"not empty");
}

fn exchange_ticket(hub_url: &[u8], ticket: &[u8]) -> PluginHttpResult {
    pandar_plugin_exchange_ticket(
        hub_url.as_ptr(),
        hub_url.len(),
        ticket.as_ptr(),
        ticket.len(),
    )
}

fn get_printers(hub_url: &[u8], token: &[u8]) -> PluginHttpResult {
    pandar_plugin_get_printers(hub_url.as_ptr(), hub_url.len(), token.as_ptr(), token.len())
}

fn get_jobs(hub_url: &[u8], token: &[u8]) -> PluginHttpResult {
    pandar_plugin_get_jobs(hub_url.as_ptr(), hub_url.len(), token.as_ptr(), token.len())
}

fn submit_print(hub_url: &[u8], token: &[u8], artifact_path: &[u8]) -> PluginHttpResult {
    let printer_id = b"printer";
    let filename = b"job.3mf";
    pandar_plugin_submit_print(
        hub_url.as_ptr(),
        hub_url.len(),
        token.as_ptr(),
        token.len(),
        printer_id.as_ptr(),
        printer_id.len(),
        filename.as_ptr(),
        filename.len(),
        artifact_path.as_ptr(),
        artifact_path.len(),
        1,
        true,
        false,
        false,
        b"".as_ptr(),
        0,
        b"".as_ptr(),
        0,
        b"".as_ptr(),
        0,
    )
}

fn submit_printer_operation(
    hub_url: &[u8],
    token: &[u8],
    operation_json: &[u8],
) -> PluginHttpResult {
    let printer_id = b"printer";
    pandar_plugin_submit_printer_operation(
        hub_url.as_ptr(),
        hub_url.len(),
        token.as_ptr(),
        token.len(),
        printer_id.as_ptr(),
        printer_id.len(),
        operation_json.as_ptr(),
        operation_json.len(),
    )
}

fn write_artifact(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
}
