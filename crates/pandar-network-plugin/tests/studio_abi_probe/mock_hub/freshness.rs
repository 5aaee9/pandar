use std::{
    net::TcpListener,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use crate::support::request_body;

use super::{
    firmware_compat, next_request,
    responses::PRINTERS_RESPONSE,
    transport::{assert_request, assert_request_with_token, read_request_until, write_response},
};

pub(super) fn serve_freshness_claim(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    race_directory: &Path,
) {
    let upgrade = super::next_stream(listener, stop, deadline);
    super::server::assert_printer_events_upgrade(&upgrade.request);
    let frames = upgrade.serve();
    for frame in super::responses::snapshot_frames(PRINTERS_RESPONSE) {
        frames.send(frame).expect("serve freshness snapshot");
    }

    let (mut version_stream, version_request) =
        read_request_until(listener, stop, deadline, "freshness claim firmware refresh")
            .expect("Studio ABI probe exited before freshness claim firmware refresh");
    assert_request_with_token(
        &version_request,
        "POST",
        "/api/v1/plugin/printers/printer-1/firmware/refresh",
        Some("probe-token"),
    );
    std::fs::create_dir(race_directory.join("freshness-version-entered")).unwrap();
    while !race_directory.join("freshness-stream-update").exists()
        && !stop.load(Ordering::Acquire)
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(5));
    }
    frames
        .send(super::stream::upsert_frame(&printer_device(
            &super::responses::printers_response_with_progress(73),
        )))
        .expect("serve freshness generation update");
    write_response(
        &mut version_stream,
        "HTTP/1.1 200 OK",
        r#"{"command_id":"00000000-0000-0000-0000-000000000099","modules":[{"name":"ota","product_name":"N6","sw_ver":"01.02.03.04","sw_new_ver":"","hw_ver":"OTA","sn":"studio-serial-1","flag":0}],"module_revision":1}"#,
    );

    while !stop.load(Ordering::Acquire) {
        let Some((mut stream, request)) =
            read_request_until(listener, stop, deadline, "freshness trailing request")
        else {
            return;
        };
        if request.starts_with("POST /api/v1/plugin/printers/printer-1/operations HTTP/1.1") {
            write_response(
                &mut stream,
                "HTTP/1.1 202 Accepted",
                r#"{"command_id":"freshness-local","status":"queued"}"#,
            );
        } else if request.starts_with("GET /healthz HTTP/1.1") {
            write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                r#"{"status":"ok","checks":{}}"#,
            );
        } else {
            panic!("unexpected freshness trailing request: {request}");
        }
    }
}

fn printer_device(response: &str) -> String {
    serde_json::from_str::<serde_json::Value>(response).unwrap()["devices"][0].to_string()
}

pub(super) fn serve_firmware_claim_race(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    race_directory: &Path,
) {
    let upgrade = super::next_stream(listener, stop, deadline);
    super::server::assert_printer_events_upgrade(&upgrade.request);
    let frames = upgrade.serve();
    for frame in super::responses::snapshot_frames(PRINTERS_RESPONSE) {
        frames.send(frame).expect("serve firmware-claim snapshot");
    }

    let prepare_path = "/api/v1/plugin/printers/printer-1/firmware/prepare";
    let (mut prepare_stream, prepare_request) =
        next_request(listener, stop, deadline, "POST", prepare_path);
    assert_request_with_token(&prepare_request, "POST", prepare_path, Some("probe-token"));
    let prepare: serde_json::Value = serde_json::from_str(request_body(&prepare_request)).unwrap();
    assert_eq!(prepare["command"], "upgrade_confirm");
    assert_eq!(prepare["sequence_id"], "c-final-claim-race");
    write_response(
        &mut prepare_stream,
        "HTTP/1.1 200 OK",
        r#"{"command_id":"00000000-0000-0000-0000-000000000011","prepared_token":"prepared-final-claim"}"#,
    );

    let execute_path = "/api/v1/plugin/printers/printer-1/firmware/execute";
    let (mut execute_stream, execute_request) =
        next_request(listener, stop, deadline, "POST", execute_path);
    assert_request_with_token(&execute_request, "POST", execute_path, Some("probe-token"));
    let execute: serde_json::Value = serde_json::from_str(request_body(&execute_request)).unwrap();
    assert_eq!(execute["prepared_token"], "prepared-final-claim");
    assert_eq!(execute["command"]["sequence_id"], "c-final-claim-race");
    std::fs::create_dir(race_directory.join("firmware-claim-execute-entered")).unwrap();
    write_response(
        &mut execute_stream,
        "HTTP/1.1 200 OK",
        r#"{"command_id":"00000000-0000-0000-0000-000000000011","phase":"rejected","outcome":{"outcome":"acknowledged","acknowledgement":{"command":"upgrade_confirm","sequence_id":"c-final-claim-race","result":"fail","err_code":765,"reason":"printer_busy","message":"selector rejected"}},"transient_status":{"upgrade_state":{"status":"FAIL","progress":"42"},"cfg":"101"}}"#,
    );
    drop(execute_stream);

    let exchange_path = "/api/v1/plugin/login-tickets/exchange";
    let (mut exchange_stream, exchange_request) =
        next_request(listener, stop, deadline, "POST", exchange_path);
    assert_request(&exchange_request, "POST", exchange_path, false);
    assert!(request_body(&exchange_request).contains("final-claim-ticket"));
    std::fs::create_dir(race_directory.join("firmware-claim-exchange-entered")).unwrap();
    thread::sleep(Duration::from_millis(1_450));
    write_response(
        &mut exchange_stream,
        "HTTP/1.1 200 OK",
        r#"{"token":"rotated-token","profile":{"token":"rotated-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"}}"#,
    );

    while !stop.load(Ordering::Acquire) {
        match super::next_incoming(listener, stop, deadline) {
            super::Incoming::Stream(upgrade) => {
                assert!(
                    upgrade
                        .request
                        .contains("authorization: Bearer rotated-token")
                );
                let frames = upgrade.serve();
                for frame in super::responses::snapshot_frames(PRINTERS_RESPONSE) {
                    frames
                        .send(frame)
                        .expect("serve rotated firmware-claim snapshot");
                }
            }
            super::Incoming::Http(mut stream, request) => {
                let line = request.lines().next().unwrap_or_default();
                let refresh_path = "/api/v1/plugin/printers/printer-1/firmware/refresh";
                if line == "POST /api/v1/plugin/printers/printer-1/firmware/refresh HTTP/1.1" {
                    assert_request_with_token(
                        &request,
                        "POST",
                        refresh_path,
                        Some("rotated-token"),
                    );
                    assert!(firmware_compat::try_respond(&mut stream, &request));
                } else if line == "GET /healthz HTTP/1.1" {
                    write_response(
                        &mut stream,
                        "HTTP/1.1 200 OK",
                        r#"{"status":"ok","checks":{}}"#,
                    );
                } else {
                    panic!("unexpected firmware final claim trailing request: {request}");
                }
            }
        }
    }
}
