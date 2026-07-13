use std::{
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use super::{
    PRINTERS_RESPONSE, firmware_compat,
    operations::{TestOperation, TestPrintErrorAction, assert_operation_body_eq},
    transport::{read_request_until, write_response},
};

fn printers_response() -> String {
    let mut response: serde_json::Value = serde_json::from_str(PRINTERS_RESPONSE).unwrap();
    let devices = response["devices"]
        .as_array_mut()
        .expect("printer response devices is an array");
    let mut cloud = devices[0].clone();
    cloud["print_error"] = serde_json::json!(83_918_929);
    cloud["job_id"] = serde_json::json!("job-7");

    let mut local_clear = cloud.clone();
    local_clear["dev_id"] = serde_json::json!("studio-serial-2");
    local_clear["pandar_printer_id"] = serde_json::json!("printer-2");
    local_clear["dev_name"] = serde_json::json!("Probe Local Printer");
    local_clear["name"] = serde_json::json!("Probe Local Printer");
    local_clear["dev_ip"] = serde_json::json!("192.0.2.11");
    local_clear["print_error"] = serde_json::json!(0);
    local_clear["job_id"] = serde_json::json!("");

    let mut local_replacement = local_clear.clone();
    local_replacement["dev_id"] = serde_json::json!("studio-serial-3");
    local_replacement["pandar_printer_id"] = serde_json::json!("printer-3");
    local_replacement["dev_name"] = serde_json::json!("Probe Replacement Printer");
    local_replacement["name"] = serde_json::json!("Probe Replacement Printer");
    local_replacement["dev_ip"] = serde_json::json!("192.0.2.12");

    *devices = vec![cloud, local_clear, local_replacement];
    response.to_string()
}

fn request_line(request: &str) -> &str {
    request.lines().next().unwrap_or_default()
}

pub(super) fn serve(listener: &TcpListener, stop: &Arc<AtomicBool>, deadline: Instant) {
    let printers = printers_response();
    let mut operation_posts = 0_u32;

    while !stop.load(Ordering::Acquire) {
        let Some((mut stream, request)) =
            read_request_until(listener, stop, deadline, "native mock Hub request")
        else {
            return;
        };
        let line = request_line(&request);

        if firmware_compat::try_respond(&mut stream, &request) {
            continue;
        }

        if line == "POST /api/v1/plugin/no-auth-session HTTP/1.1" {
            write_response(
                &mut stream,
                "HTTP/1.1 403 Forbidden",
                r#"{"error":"no_auth_required"}"#,
            );
        } else if line == "POST /api/v1/plugin/login-tickets/exchange HTTP/1.1" {
            write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                r#"{"token":"probe-token","profile":{"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"}}"#,
            );
        } else if line == "GET /api/v1/plugin/printers HTTP/1.1" {
            assert!(
                request.contains("authorization: Bearer probe-token"),
                "native probe printer refresh omitted bearer token: {request}"
            );
            write_response(&mut stream, "HTTP/1.1 200 OK", &printers);
        } else if line == "GET /probe-operation-count HTTP/1.1" {
            write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                &serde_json::json!({"count": operation_posts}).to_string(),
            );
        } else if line.starts_with("POST /api/v1/plugin/printers/")
            && line.contains("/operations HTTP/1.1")
        {
            let (printer_id, error_action, printer_job_id, sequence_id) = match operation_posts {
                0 => (
                    "printer-1",
                    TestPrintErrorAction::Resume,
                    "cloud-resume-get_version-pushall",
                    20_042,
                ),
                1 => (
                    "printer-1",
                    TestPrintErrorAction::Ignore,
                    "cloud-ignore-get_version-pushall",
                    20_043,
                ),
                2 => (
                    "printer-1",
                    TestPrintErrorAction::Stop,
                    "cloud-stop-get_version-pushall",
                    20_044,
                ),
                3 => (
                    "printer-2",
                    TestPrintErrorAction::Resume,
                    "local-resume-get_version-pushall",
                    20_045,
                ),
                4 => (
                    "printer-2",
                    TestPrintErrorAction::Ignore,
                    "local-ignore-get_version-pushall",
                    20_046,
                ),
                5 => (
                    "printer-2",
                    TestPrintErrorAction::Stop,
                    "local-stop-get_version-pushall",
                    20_047,
                ),
                _ => panic!("unexpected extra native operation: {request}"),
            };
            assert_eq!(
                line,
                format!("POST /api/v1/plugin/printers/{printer_id}/operations HTTP/1.1")
            );
            assert_operation_body_eq(
                &request,
                TestOperation::HandlePrintError {
                    error_action,
                    print_error: 83_918_929,
                    printer_job_id: printer_job_id.to_owned(),
                    sequence_id,
                },
            );
            operation_posts += 1;
            write_response(
                &mut stream,
                "HTTP/1.1 202 Accepted",
                r#"{"command_id":"native-command","status":"sent"}"#,
            );
        } else {
            panic!("unexpected native probe request: {request}");
        }
    }
}
