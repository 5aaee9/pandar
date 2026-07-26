use std::{
    fs,
    net::TcpListener,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Instant,
};

use super::{
    next_request,
    responses::{PRINTERS_RESPONSE, filament_switch_printers_response},
    transport::{assert_request, assert_request_with_token, read_request_until, write_response},
};

pub(super) fn serve_connection_readiness(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
) {
    let responses = [
        (
            "HTTP/1.1 503 Service Unavailable",
            r#"{"status":"not_ready","checks":{}}"#,
        ),
        (
            "HTTP/1.1 503 Service Unavailable",
            r#"{"status":"not_ready","checks":{}}"#,
        ),
        ("HTTP/1.1 200 OK", r#"{"status":"ok","checks":{}}"#),
        ("HTTP/1.1 200 OK", r#"{"status":"ok","checks":{}}"#),
        ("HTTP/1.1 200 OK", r#"{"status":"ok"#),
        (
            "HTTP/1.1 503 Service Unavailable",
            r#"{"status":"not_ready","checks":{}}"#,
        ),
        ("HTTP/1.1 200 OK", r#"{"status":"ok","checks":{}}"#),
    ];
    for (status, body) in responses {
        let Some((mut stream, request)) =
            read_request_until(listener, stop, deadline, "connection readiness request")
        else {
            return;
        };
        assert_request(&request, "GET", "/healthz", false);
        write_response(&mut stream, status, body);
    }
}

pub(super) fn serve_no_auth_recovery(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    race_directory: &Path,
    zero_touch: bool,
    allow_session_delete: bool,
) {
    let mut no_auth_posts = 0;
    let mut issued_tokens = 0;
    let mut audit_records = 0;
    let mut printer_reads = 0;

    loop {
        let Some((mut stream, request)) = super::transport::read_request_until(
            listener,
            stop,
            deadline,
            "no-auth recovery request",
        ) else {
            return;
        };
        let line = request.lines().next().unwrap_or_default();
        match line {
            "POST /api/v1/plugin/no-auth-session HTTP/1.1" => {
                no_auth_posts += 1;
                issued_tokens += 1;
                audit_records += 1;
                write_response(
                    &mut stream,
                    "HTTP/1.1 200 OK",
                    r#"{"token":"recovered-token","profile":{"token":"recovered-token","user_id":"recovered-user","user_name":"Recovered User","tenant_id":"tenant-1","tenant_name":"Tenant"}}"#,
                );
            }
            "GET /api/v1/plugin/printers HTTP/1.1" => {
                assert_request_with_token(
                    &request,
                    "GET",
                    "/api/v1/plugin/printers",
                    Some("recovered-token"),
                );
                printer_reads += 1;
                write_response(
                    &mut stream,
                    "HTTP/1.1 200 OK",
                    r#"{"message":"success","devices":[]}"#,
                );
                if zero_touch {
                    break;
                }
            }
            "GET /healthz HTTP/1.1" => {
                write_response(
                    &mut stream,
                    "HTTP/1.1 200 OK",
                    r#"{"status":"ok","checks":{}}"#,
                );
                break;
            }
            "DELETE /api/v1/plugin/session HTTP/1.1" if allow_session_delete => {
                write_response(&mut stream, "HTTP/1.1 204 No Content", "");
            }
            _ => panic!("unexpected no-auth recovery request: {line}"),
        }
    }

    assert_eq!(
        no_auth_posts, 1,
        "no-auth recovery repeated session creation"
    );
    assert_eq!(issued_tokens, 1, "no-auth recovery issued duplicate tokens");
    assert_eq!(
        audit_records, 1,
        "no-auth recovery wrote duplicate audit records"
    );
    assert_eq!(
        printer_reads, 1,
        "no-auth recovery repeated the initial printer refresh"
    );
    fs::write(
        race_directory.join("no-auth-recovery-counts"),
        format!(
            "posts={no_auth_posts} tokens={issued_tokens} audits={audit_records} printers={printer_reads}"
        ),
    )
    .unwrap();
}

pub(super) fn serve_background_timeout(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
) {
    let (mut stream, request) =
        next_request(listener, stop, deadline, "GET", "/api/v1/plugin/printers");
    assert_request_with_token(
        &request,
        "GET",
        "/api/v1/plugin/printers",
        Some("probe-token"),
    );
    write_response(&mut stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE);

    let (mut stream, request) = next_request(listener, stop, deadline, "GET", "/healthz");
    assert_request(&request, "GET", "/healthz", false);
    write_response(
        &mut stream,
        "HTTP/1.1 200 OK",
        r#"{"status":"ok","checks":{}}"#,
    );

    let (_stream, request) =
        next_request(listener, stop, deadline, "GET", "/api/v1/plugin/printers");
    assert_request_with_token(
        &request,
        "GET",
        "/api/v1/plugin/printers",
        Some("probe-token"),
    );
    thread::sleep(std::time::Duration::from_millis(1_100));
}

pub(super) fn serve_auth_rejected(listener: &TcpListener, stop: &AtomicBool, deadline: Instant) {
    let (mut stream, request) =
        next_request(listener, stop, deadline, "GET", "/api/v1/plugin/printers");
    assert_request_with_token(
        &request,
        "GET",
        "/api/v1/plugin/printers",
        Some("probe-token"),
    );
    write_response(&mut stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE);

    let (mut stream, request) = next_request(listener, stop, deadline, "GET", "/healthz");
    assert_request(&request, "GET", "/healthz", false);
    write_response(
        &mut stream,
        "HTTP/1.1 200 OK",
        r#"{"status":"ok","checks":{}}"#,
    );

    let (mut stream, request) =
        next_request(listener, stop, deadline, "GET", "/api/v1/plugin/printers");
    assert_request_with_token(
        &request,
        "GET",
        "/api/v1/plugin/printers",
        Some("probe-token"),
    );
    write_response(
        &mut stream,
        "HTTP/1.1 401 Unauthorized",
        r#"{"error":"invalid_auth_token"}"#,
    );

    for index in 0..2 {
        let Some((mut stream, request)) = read_request_until(
            listener,
            stop,
            deadline,
            "authenticated rejection printer refresh",
        ) else {
            assert_ne!(
                index, 0,
                "repeated authenticated rejection refresh was missing"
            );
            return;
        };
        assert_request_with_token(
            &request,
            "GET",
            "/api/v1/plugin/printers",
            Some("probe-token"),
        );
        write_response(
            &mut stream,
            "HTTP/1.1 403 Forbidden",
            r#"{"error":"invalid_auth_token"}"#,
        );
    }
}

pub(super) fn serve_account_transition(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
) {
    while !stop.load(Ordering::Acquire) {
        let Some((mut stream, request)) = read_request_until(
            listener,
            stop,
            deadline,
            "account transition printer refresh",
        ) else {
            return;
        };
        assert_request_with_token(
            &request,
            "GET",
            "/api/v1/plugin/printers",
            Some("probe-token"),
        );
        write_response(&mut stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE);
    }
}

pub(super) fn serve_token_rotation(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    offline_retry: bool,
) {
    let mut steps = vec![
        ("GET", "/api/v1/plugin/printers", Some("stale-token")),
        ("POST", "/api/v1/plugin/no-auth-session", None),
        ("GET", "/api/v1/plugin/printers", Some("probe-token")),
        ("GET", "/api/v1/plugin/printers", Some("probe-token")),
        ("POST", "/api/v1/plugin/no-auth-session", None),
        ("GET", "/api/v1/plugin/printers", Some("rotated-token")),
    ];
    steps.push(("GET", "/api/v1/plugin/printers", Some("rotated-token")));
    for (index, (method, path, token)) in steps.into_iter().enumerate() {
        let (mut stream, request) = next_request(listener, stop, deadline, method, path);
        assert_request_with_token(&request, method, path, token);
        match index {
            0 | 3 => write_response(
                &mut stream,
                "HTTP/1.1 401 Unauthorized",
                r#"{"error":"token_expired"}"#,
            ),
            1 => write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                r#"{"token":"probe-token","profile":{"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"}}"#,
            ),
            4 => write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                r#"{"token":"rotated-token","profile":{"token":"rotated-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"}}"#,
            ),
            2 => write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                &filament_switch_printers_response(),
            ),
            5 | 6 if offline_retry => write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                r#"{"message":"success","devices":[]}"#,
            ),
            5 => write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                &filament_switch_printers_response(),
            ),
            6 => write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                r#"{"message":"success","devices":["invalid"]}"#,
            ),
            _ => unreachable!(),
        }
    }
}
