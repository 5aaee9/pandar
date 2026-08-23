use std::{
    net::TcpListener,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use super::{
    firmware_compat,
    operations::{AxisFeatureOperation, assert_axis_feature_operation_body_eq},
    responses::{PRINTERS_RESPONSE, axis_printers_response, printers_response_with_progress},
    transport::write_response,
};

pub(super) fn serve_axis_features(listener: &TcpListener, stop: &AtomicBool, deadline: Instant) {
    let mut operation_posts = 0_u32;
    while !stop.load(Ordering::Acquire) {
        match super::next_incoming(listener, stop, deadline) {
            super::Incoming::Stream(upgrade) => {
                super::server::assert_printer_events_upgrade(&upgrade.request);
                let frames = upgrade.serve();
                for frame in super::responses::snapshot_frames(&axis_printers_response()) {
                    frames.send(frame).expect("serve axis feature snapshot");
                }
            }
            super::Incoming::Http(mut stream, request) => {
                let line = request.lines().next().unwrap_or_default();
                if firmware_compat::try_respond(&mut stream, &request) {
                    continue;
                }
                if line == "GET /healthz HTTP/1.1" {
                    write_response(
                        &mut stream,
                        "HTTP/1.1 200 OK",
                        r#"{"status":"ok","checks":{}}"#,
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
    }
}

pub(super) fn serve_printer_presence(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    race_directory: &Path,
) {
    let upgrade = super::next_stream(listener, stop, deadline);
    super::server::assert_printer_events_upgrade(&upgrade.request);
    let mut frames = Some(upgrade.serve());
    for frame in super::snapshot_script(&[]) {
        frames.as_ref().unwrap().send(frame).unwrap();
    }
    let mut printer_step = 0_u32;
    let mut reconnecting = false;
    while !stop.load(Ordering::Acquire) {
        match super::next_incoming(listener, stop, deadline) {
            super::Incoming::Stream(upgrade) => {
                super::server::assert_printer_events_upgrade(&upgrade.request);
                let sender = upgrade.serve();
                for frame in super::responses::snapshot_frames(&printers_response_with_progress(73))
                {
                    sender
                        .send(frame)
                        .expect("serve presence recovery snapshot");
                }
                frames = Some(sender);
                reconnecting = false;
                printer_step = 3;
            }
            super::Incoming::Http(mut stream, request) => {
                let line = request.lines().next().unwrap_or_default();
                if line == "GET /probe-presence-step HTTP/1.1" {
                    if race_directory.join("presence-online").exists() && printer_step == 0 {
                        frames
                            .as_ref()
                            .unwrap()
                            .send(super::stream::upsert_frame(&printer_device(
                                PRINTERS_RESPONSE,
                            )))
                            .unwrap();
                        printer_step = 1;
                    } else if race_directory.join("presence-offline").exists() && printer_step == 1
                    {
                        let offline = printer_device(PRINTERS_RESPONSE)
                            .replacen(r#""dev_online":true"#, r#""dev_online":false"#, 1)
                            .replacen(r#""online":true"#, r#""online":false"#, 1);
                        frames
                            .as_ref()
                            .unwrap()
                            .send(super::stream::upsert_frame(&offline))
                            .unwrap();
                        printer_step = 2;
                    } else if race_directory.join("presence-reconnect").exists()
                        && printer_step == 2
                        && !reconnecting
                    {
                        reconnecting = true;
                        frames.take().unwrap().send("@close".to_owned()).unwrap();
                    } else if race_directory.join("presence-remove").exists() && printer_step == 3 {
                        frames
                            .as_ref()
                            .unwrap()
                            .send(
                                r#"{"type":"printer_removed","dev_id":"studio-serial-1","pandar_printer_id":"printer-1"}"#
                                    .to_owned(),
                            )
                            .unwrap();
                        printer_step = 4;
                    }
                    write_response(
                        &mut stream,
                        "HTTP/1.1 200 OK",
                        &serde_json::json!({"step": printer_step}).to_string(),
                    );
                    if race_directory.join("presence-done").exists() {
                        return;
                    }
                } else if line == "GET /healthz HTTP/1.1" {
                    write_response(
                        &mut stream,
                        "HTTP/1.1 200 OK",
                        r#"{"status":"ok","checks":{}}"#,
                    );
                } else if firmware_compat::try_respond(&mut stream, &request) {
                    continue;
                } else {
                    panic!("unexpected printer presence request: {request}");
                }
            }
        }
    }
}

fn printer_device(response: &str) -> String {
    serde_json::from_str::<serde_json::Value>(response).unwrap()["devices"][0].to_string()
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
    let upgrade = super::next_stream(listener, stop, deadline);
    super::server::assert_printer_events_upgrade(&upgrade.request);
    let frames = upgrade.serve();
    for frame in super::responses::snapshot_frames(PRINTERS_RESPONSE) {
        frames.send(frame).expect("serve callback-order snapshot");
    }

    wait_for_race_marker(
        &race_directory.join("callback-order-offline-request"),
        stop,
        deadline,
        "callback-order probe did not request the offline update",
    );
    std::fs::create_dir(race_directory.join("callback-order-offline-entered")).unwrap();
    let offline = printer_device(PRINTERS_RESPONSE)
        .replacen(r#""dev_online":true"#, r#""dev_online":false"#, 1)
        .replacen(r#""online":true"#, r#""online":false"#, 1);
    frames
        .send(super::stream::upsert_frame(&offline))
        .expect("serve callback-order offline update");
    std::fs::create_dir(race_directory.join("callback-order-offline-responded")).unwrap();

    wait_for_race_marker(
        &race_directory.join("callback-order-recovery-request"),
        stop,
        deadline,
        "callback-order probe did not request recovery",
    );
    std::fs::create_dir(race_directory.join("callback-order-recovery-entered")).unwrap();
    frames
        .send(super::stream::upsert_frame(&printer_device(
            &printers_response_with_progress(73),
        )))
        .expect("serve callback-order recovery update");
    std::fs::create_dir(race_directory.join("callback-order-recovery-responded")).unwrap();
}
