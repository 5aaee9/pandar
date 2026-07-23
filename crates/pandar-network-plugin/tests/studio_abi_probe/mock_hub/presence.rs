use std::{
    net::TcpListener,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use super::{
    firmware_compat, next_request,
    operations::{AxisFeatureOperation, assert_axis_feature_operation_body_eq},
    responses::{PRINTERS_RESPONSE, axis_printers_response, printers_response_with_progress},
    transport::{assert_request_with_token, read_request_until, write_response},
};

pub(super) fn serve_axis_features(listener: &TcpListener, stop: &AtomicBool, deadline: Instant) {
    let mut operation_posts = 0_u32;
    while !stop.load(Ordering::Acquire) {
        let Some((mut stream, request)) =
            read_request_until(listener, stop, deadline, "axis feature probe request")
        else {
            return;
        };
        let line = request.lines().next().unwrap_or_default();
        if firmware_compat::try_respond(&mut stream, &request) {
            continue;
        }
        if line == "GET /readyz HTTP/1.1" {
            write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                r#"{"status":"ready","checks":{}}"#,
            );
        } else if line == "POST /api/v1/plugin/no-auth-session HTTP/1.1" {
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
            write_response(&mut stream, "HTTP/1.1 200 OK", &axis_printers_response());
        } else if line == "GET /probe-operation-count HTTP/1.1" {
            write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                &serde_json::json!({"count": operation_posts}).to_string(),
            );
        } else if line == "POST /api/v1/plugin/printers/printer-1/operations HTTP/1.1"
            || line == "POST /api/v1/plugin/printers/printer-2/operations HTTP/1.1"
        {
            let printer_id = if operation_posts < 5 {
                "printer-1"
            } else {
                "printer-2"
            };
            assert_eq!(
                line,
                format!("POST /api/v1/plugin/printers/{printer_id}/operations HTTP/1.1")
            );
            let expected = match operation_posts {
                0 | 5 => AxisFeatureOperation::modern_home(),
                1 => AxisFeatureOperation::modern_move("x", 1.0),
                2 | 7 => AxisFeatureOperation::legacy_home(),
                3 => AxisFeatureOperation::legacy_move("x", 10.0, 3000),
                4 => AxisFeatureOperation::gcode_line("M106 P1 S127 \n"),
                6 => AxisFeatureOperation::modern_move("z", -10.0),
                8 => AxisFeatureOperation::legacy_move("z", -1.0, 600),
                9 => AxisFeatureOperation::gcode_line("M620 C1 \r\n; keep trailing  \n\n"),
                _ => panic!("unexpected extra axis feature operation: {request}"),
            };
            assert_axis_feature_operation_body_eq(&request, expected);
            operation_posts += 1;
            write_response(
                &mut stream,
                "HTTP/1.1 202 Accepted",
                r#"{"command_id":"axis-command","status":"sent"}"#,
            );
        } else {
            panic!("unexpected axis feature probe request: {request}");
        }
    }
}

pub(super) fn serve_printer_presence(listener: &TcpListener, stop: &AtomicBool, deadline: Instant) {
    let mut printer_step = 0_u32;
    while !stop.load(Ordering::Acquire) {
        let Some((mut stream, request)) =
            read_request_until(listener, stop, deadline, "printer presence request")
        else {
            return;
        };
        let line = request.lines().next().unwrap_or_default();
        if line == "GET /readyz HTTP/1.1" {
            write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                r#"{"status":"ready","checks":{}}"#,
            );
        } else if line == "GET /probe-presence-step HTTP/1.1" {
            write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                &serde_json::json!({"step": printer_step}).to_string(),
            );
        } else if line == "GET /api/v1/plugin/printers HTTP/1.1" {
            assert_request_with_token(
                &request,
                "GET",
                "/api/v1/plugin/printers",
                Some("probe-token"),
            );
            printer_step += 1;
            match printer_step {
                1 | 5 | 6 => write_response(&mut stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE),
                2 => write_response(
                    &mut stream,
                    "HTTP/1.1 200 OK",
                    &PRINTERS_RESPONSE.replacen(r#""dev_online":true"#, r#""dev_online":false"#, 1),
                ),
                3 => thread::sleep(Duration::from_millis(1_100)),
                4 => write_response(
                    &mut stream,
                    "HTTP/1.1 500 Internal Server Error",
                    r#"{"error":"printer_refresh_failed"}"#,
                ),
                _ => panic!("unexpected printer presence refresh: {request}"),
            }
        } else if firmware_compat::try_respond(&mut stream, &request) {
            continue;
        } else {
            panic!("unexpected printer presence request: {request}");
        }
    }
}

fn wait_for_race_marker(marker: &Path, stop: &AtomicBool, deadline: Instant, description: &str) {
    while !marker.exists() && !stop.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(marker.exists(), "{description}");
}

pub(super) fn serve_callback_order(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    race_directory: &Path,
) {
    let (mut seed_stream, seed_request) =
        next_request(listener, stop, deadline, "GET", "/api/v1/plugin/printers");
    assert_request_with_token(
        &seed_request,
        "GET",
        "/api/v1/plugin/printers",
        Some("probe-token"),
    );
    write_response(&mut seed_stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE);

    let (mut offline_stream, offline_request) =
        next_request(listener, stop, deadline, "GET", "/api/v1/plugin/printers");
    assert_request_with_token(
        &offline_request,
        "GET",
        "/api/v1/plugin/printers",
        Some("probe-token"),
    );
    std::fs::create_dir(race_directory.join("callback-order-offline-entered")).unwrap();
    wait_for_race_marker(
        &race_directory.join("callback-order-release-offline"),
        stop,
        deadline,
        "callback-order probe did not release the offline refresh",
    );
    let offline_response = PRINTERS_RESPONSE
        .replacen(r#""dev_online":true"#, r#""dev_online":false"#, 1)
        .replacen(r#""online":true"#, r#""online":false"#, 1);
    write_response(&mut offline_stream, "HTTP/1.1 200 OK", &offline_response);
    std::fs::create_dir(race_directory.join("callback-order-offline-responded")).unwrap();

    let (mut recovery_stream, recovery_request) =
        next_request(listener, stop, deadline, "GET", "/api/v1/plugin/printers");
    assert_request_with_token(
        &recovery_request,
        "GET",
        "/api/v1/plugin/printers",
        Some("probe-token"),
    );
    std::fs::create_dir(race_directory.join("callback-order-recovery-entered")).unwrap();
    wait_for_race_marker(
        &race_directory.join("callback-order-release-recovery"),
        stop,
        deadline,
        "callback-order probe did not release the recovery refresh",
    );
    write_response(
        &mut recovery_stream,
        "HTTP/1.1 200 OK",
        &printers_response_with_progress(73),
    );
    std::fs::create_dir(race_directory.join("callback-order-recovery-responded")).unwrap();
}
