use std::{
    net::TcpListener,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use crate::support::request_body;

use super::{
    next_request,
    responses::PRINTERS_RESPONSE,
    transport::{assert_request, assert_request_with_token, write_response},
};

pub(super) fn serve_account_exchange_race(
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
        Some("account-a-token"),
    );
    write_response(&mut seed_stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE);

    let exchange_path = "/api/v1/plugin/login-tickets/exchange";
    let (mut exchange_stream, exchange_request) =
        next_request(listener, stop, deadline, "POST", exchange_path);
    assert_request(&exchange_request, "POST", exchange_path, false);
    assert!(request_body(&exchange_request).contains("serialized-stale-ticket"));
    std::fs::create_dir(race_directory.join("account-exchange-entered")).unwrap();

    let release = race_directory.join("account-exchange-release");
    while !release.exists() && Instant::now() < deadline && !stop.load(Ordering::Acquire) {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        release.exists(),
        "probe did not release the blocked ticket exchange"
    );
    write_response(
        &mut exchange_stream,
        "HTTP/1.1 200 OK",
        r#"{"token":"stale-response-token","profile":{"token":"stale-response-token","user_id":"stale-response-user","user_name":"Stale Response User","tenant_id":"tenant-stale","tenant_name":"Stale Tenant"}}"#,
    );

    let mut stale_revokes = 0;
    let mut replacement_refreshes = 0;
    for _ in 0..2 {
        let (mut stream, request) = next_request(
            listener,
            stop,
            deadline,
            "DELETE or GET",
            "stale candidate cleanup or replacement refresh",
        );
        if request.starts_with("DELETE /api/v1/plugin/session HTTP/1.1\r\n") {
            stale_revokes += 1;
            assert_request_with_token(
                &request,
                "DELETE",
                "/api/v1/plugin/session",
                Some("stale-response-token"),
            );
            write_response(&mut stream, "HTTP/1.1 204 No Content", "");
        } else if request.starts_with("GET /api/v1/plugin/printers HTTP/1.1\r\n") {
            replacement_refreshes += 1;
            assert_request_with_token(
                &request,
                "GET",
                "/api/v1/plugin/printers",
                Some("account-b-token"),
            );
            write_response(&mut stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE);
        } else {
            panic!("unexpected account race request: {request}");
        }
    }
    assert_eq!(
        stale_revokes, 1,
        "stale ticket candidate was not revoked once"
    );
    assert_eq!(
        replacement_refreshes, 1,
        "replacement account did not refresh printers once"
    );

    let (mut fifo_stream, fifo_request) =
        next_request(listener, stop, deadline, "POST", exchange_path);
    assert_request(&fifo_request, "POST", exchange_path, false);
    assert!(request_body(&fifo_request).contains("fifo-login-ticket"));
    std::fs::create_dir(race_directory.join("account-fifo-exchange-entered")).unwrap();

    let fifo_release = race_directory.join("account-fifo-exchange-release");
    while !fifo_release.exists() && Instant::now() < deadline && !stop.load(Ordering::Acquire) {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        fifo_release.exists(),
        "probe did not release the FIFO login exchange"
    );
    write_response(
        &mut fifo_stream,
        "HTTP/1.1 200 OK",
        r#"{"token":"fifo-login-token","profile":{"token":"fifo-login-token","user_id":"fifo-login-user","user_name":"FIFO Login User","tenant_id":"tenant-fifo","tenant_name":"FIFO Tenant"}}"#,
    );

    let (mut fifo_revoke, fifo_revoke_request) =
        next_request(listener, stop, deadline, "DELETE", "/api/v1/plugin/session");
    assert_request_with_token(
        &fifo_revoke_request,
        "DELETE",
        "/api/v1/plugin/session",
        Some("fifo-login-token"),
    );
    write_response(
        &mut fifo_revoke,
        "HTTP/1.1 503 Service Unavailable",
        r#"{"error":"revoke_unavailable"}"#,
    );
}
