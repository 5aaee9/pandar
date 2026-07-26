use std::{
    net::TcpListener,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use crate::support::{read_http_request_with_timeout, request_body};

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
    let (mut seed_stream, seed_request) =
        next_request(listener, stop, deadline, "GET", "/api/v1/plugin/printers");
    assert_request_with_token(
        &seed_request,
        "GET",
        "/api/v1/plugin/printers",
        Some("probe-token"),
    );
    write_response(&mut seed_stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE);

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

    let (mut refresh_stream, refresh_request) =
        next_request(listener, stop, deadline, "GET", "/api/v1/plugin/printers");
    assert_request_with_token(
        &refresh_request,
        "GET",
        "/api/v1/plugin/printers",
        Some("probe-token"),
    );
    std::fs::create_dir(race_directory.join("freshness-printer-entered")).unwrap();
    write_response(
        &mut version_stream,
        "HTTP/1.1 200 OK",
        r#"{"command_id":"00000000-0000-0000-0000-000000000099","modules":[{"name":"ota","product_name":"N6","sw_ver":"01.02.03.04","sw_new_ver":"","hw_ver":"OTA","sn":"studio-serial-1","flag":0}],"module_revision":1}"#,
    );

    let local_send_returned = race_directory.join("freshness-local-send-returned");
    let version_returned = race_directory.join("freshness-version-returned");
    let connect_checked = race_directory.join("freshness-connect-checked");
    let mut connection_ready_seen = false;
    while !(local_send_returned.exists() && version_returned.exists() && connect_checked.exists())
        && Instant::now() < deadline
        && !stop.load(Ordering::Acquire)
    {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                let request =
                    read_http_request_with_timeout(&mut stream, Some(Duration::from_millis(500)));
                if request.starts_with("GET /healthz HTTP/1.1") {
                    assert_request(&request, "GET", "/healthz", false);
                    connection_ready_seen = true;
                    write_response(
                        &mut stream,
                        "HTTP/1.1 200 OK",
                        r#"{"status":"ok","checks":{}}"#,
                    );
                } else {
                    assert_request_with_token(
                        &request,
                        "POST",
                        "/api/v1/plugin/printers/printer-1/operations",
                        Some("probe-token"),
                    );
                    std::fs::create_dir(race_directory.join("freshness-local-operation-received"))
                        .unwrap();
                    write_response(
                        &mut stream,
                        "HTTP/1.1 202 Accepted",
                        r#"{"command_id":"freshness-local","status":"queued"}"#,
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("freshness claim accept failed: {error}"),
        }
    }
    assert!(
        local_send_returned.exists(),
        "local send did not return while printer refresh was in flight"
    );
    assert!(
        version_returned.exists(),
        "version request did not return while printer refresh was in flight"
    );
    assert!(
        connect_checked.exists(),
        "connect_server admission ownership was not checked before printer refresh release"
    );
    write_response(&mut refresh_stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE);

    if !connection_ready_seen {
        let (mut ready_stream, ready_request) =
            next_request(listener, stop, deadline, "GET", "/healthz");
        assert_request(&ready_request, "GET", "/healthz", false);
        write_response(
            &mut ready_stream,
            "HTTP/1.1 200 OK",
            r#"{"status":"ok","checks":{}}"#,
        );
    }

    let (mut retry_stream, retry_request) =
        next_request(listener, stop, deadline, "GET", "/api/v1/plugin/printers");
    assert_request_with_token(
        &retry_request,
        "GET",
        "/api/v1/plugin/printers",
        Some("probe-token"),
    );
    write_response(&mut retry_stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE);
}

pub(super) fn serve_firmware_claim_race(
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
        let Some((mut stream, request)) = read_request_until(
            listener,
            stop,
            deadline,
            "firmware final claim trailing request",
        ) else {
            return;
        };
        let line = request.lines().next().unwrap_or_default();
        let firmware_refresh_path = "/api/v1/plugin/printers/printer-1/firmware/refresh";
        if line == "POST /api/v1/plugin/printers/printer-1/firmware/refresh HTTP/1.1" {
            assert_request_with_token(
                &request,
                "POST",
                firmware_refresh_path,
                Some("rotated-token"),
            );
            assert!(firmware_compat::try_respond(&mut stream, &request));
        } else if line == "GET /healthz HTTP/1.1" {
            write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                r#"{"status":"ok","checks":{}}"#,
            );
        } else if line == "GET /api/v1/plugin/printers HTTP/1.1" {
            assert_request_with_token(
                &request,
                "GET",
                "/api/v1/plugin/printers",
                Some("rotated-token"),
            );
            write_response(&mut stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE);
        } else {
            panic!("unexpected firmware final claim trailing request: {request}");
        }
    }
}
