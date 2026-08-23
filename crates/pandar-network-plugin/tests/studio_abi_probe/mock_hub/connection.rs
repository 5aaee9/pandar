use super::{
    Incoming, next_incoming, next_stream,
    responses::{filament_switch_printers_response, snapshot_frames},
    server::assert_printer_events_upgrade,
    snapshot_script,
    transport::{assert_request_with_token, write_response},
};
use std::{
    fs,
    net::TcpListener,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

pub(super) fn serve_connection_readiness(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    race_directory: &Path,
) {
    let upgrade = next_stream(listener, stop, deadline);
    assert_request_with_token(
        &upgrade.request,
        "GET",
        "/api/v1/tenants/tenant-1/printer-events?projection=studio&version=1",
        Some("probe-token"),
    );
    let frames = upgrade.serve();
    let release = race_directory.join("stream-release-snapshot");
    while !release.exists() && !stop.load(Ordering::Acquire) && Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                let request = crate::support::read_http_request_with_timeout(
                    &mut stream,
                    Some(Duration::from_secs(5)),
                );
                assert_eq!(request.lines().next(), Some("GET /healthz HTTP/1.1"));
                write_response(
                    &mut stream,
                    "HTTP/1.1 200 OK",
                    r#"{"status":"ok","checks":{}}"#,
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("readiness mock accept failed: {error}"),
        }
    }
    assert!(release.exists(), "readiness snapshot was not released");
    for frame in snapshot_frames(&filament_switch_printers_response()) {
        frames.send(frame).expect("serve readiness snapshot");
    }
}

pub(super) fn serve_no_auth_recovery(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    race_directory: &Path,
    _zero_touch: bool,
    allow_session_delete: bool,
) {
    let mut no_auth_posts = 0;
    let mut issued_tokens = 0;
    let mut audit_records = 0;
    let mut stream_upgrades = 0;

    loop {
        let incoming = super::next_incoming(listener, stop, deadline);
        match incoming {
            super::Incoming::Stream(upgrade) => {
                assert_request_with_token(
                    &upgrade.request,
                    "GET",
                    "/api/v1/tenants/tenant-1/printer-events?projection=studio&version=1",
                    Some("recovered-token"),
                );
                stream_upgrades += 1;
                let frames = upgrade.serve();
                for frame in snapshot_script(&[]) {
                    frames.send(frame).expect("serve no-auth recovery snapshot");
                }
            }
            super::Incoming::Http(mut stream, request) => {
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
        }
        if stream_upgrades > 0 {
            break;
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
        stream_upgrades, 1,
        "no-auth recovery repeated the printer-events stream dial"
    );
    fs::write(
        race_directory.join("no-auth-recovery-counts"),
        format!(
            "posts={no_auth_posts} tokens={issued_tokens} audits={audit_records} upgrades={stream_upgrades}"
        ),
    )
    .unwrap();
}

pub(super) fn serve_background_timeout(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    race_directory: &Path,
) {
    let upgrade = next_stream(listener, stop, deadline);
    assert_printer_events_upgrade(&upgrade.request);
    let frames = upgrade.serve();
    for frame in snapshot_frames(&filament_switch_printers_response()) {
        frames
            .send(frame)
            .expect("serve background timeout snapshot");
    }
    wait_for_marker(stop, race_directory, "stream-drop-now");
    frames.send("@close".to_owned()).expect("drop stream");
    let upgrade = next_stream(listener, stop, deadline);
    assert_printer_events_upgrade(&upgrade.request);
    let frames = upgrade.serve();
    for frame in snapshot_frames(&filament_switch_printers_response()) {
        frames.send(frame).expect("serve redialed snapshot");
    }
    std::fs::write(race_directory.join("stream-redial-served"), "served").unwrap();
    wait_for_marker(stop, race_directory, "stream-go-dark");
    frames.send("@close".to_owned()).expect("go dark");
    loop {
        if stop.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        match next_incoming(listener, stop, deadline) {
            Incoming::Stream(upgrade) => upgrade.reject(
                "HTTP/1.1 503 Service Unavailable",
                r#"{"error":"stream_unavailable"}"#,
            ),
            Incoming::Http(mut stream, request) => {
                if request.lines().next() == Some("GET /healthz HTTP/1.1") {
                    write_response(
                        &mut stream,
                        "HTTP/1.1 503 Service Unavailable",
                        r#"{"status":"not_ready","checks":{}}"#,
                    );
                } else {
                    panic!("unexpected background-timeout request: {request}");
                }
            }
        }
    }
}

pub(super) fn serve_stream_unavailable(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    race_directory: &Path,
) {
    let upgrade = next_stream(listener, stop, deadline);
    assert_printer_events_upgrade(&upgrade.request);
    let frames = upgrade.serve();
    for frame in snapshot_frames(&filament_switch_printers_response()) {
        frames
            .send(frame)
            .expect("serve stream-unavailable snapshot");
    }
    wait_for_marker(stop, race_directory, "stream-go-dark");
    frames
        .send("@close".to_owned())
        .expect("close unhealthy stream");

    loop {
        match next_incoming(listener, stop, deadline) {
            Incoming::Stream(upgrade) => {
                assert_printer_events_upgrade(&upgrade.request);
                if race_directory.join("stream-healthy-recover").exists() {
                    let frames = upgrade.serve();
                    for frame in snapshot_frames(&filament_switch_printers_response()) {
                        frames.send(frame).expect("serve healthy stream recovery");
                    }
                    std::fs::write(
                        race_directory.join("stream-healthy-recovery-served"),
                        "served",
                    )
                    .unwrap();
                    return;
                }
                upgrade.reject(
                    "HTTP/1.1 503 Service Unavailable",
                    r#"{"error":"stream_unavailable"}"#,
                );
            }
            Incoming::Http(mut stream, request) => {
                if request.lines().next() == Some("GET /healthz HTTP/1.1") {
                    thread::sleep(Duration::from_millis(1_100));
                    write_response(
                        &mut stream,
                        "HTTP/1.1 200 OK",
                        r#"{"status":"ok","checks":{}}"#,
                    );
                } else {
                    panic!("unexpected stream-unavailable request: {request}");
                }
            }
        }
    }
}

pub(super) fn serve_auth_rejected(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    race_directory: &Path,
) {
    let upgrade = next_stream(listener, stop, deadline);
    assert_printer_events_upgrade(&upgrade.request);
    let frames = upgrade.serve();
    for frame in snapshot_frames(&filament_switch_printers_response()) {
        frames.send(frame).expect("serve auth-rejected snapshot");
    }
    wait_for_marker(stop, race_directory, "stream-drop-now");
    frames.send("@close".to_owned()).expect("drop stream");

    let upgrade = next_stream(listener, stop, deadline);
    assert_printer_events_upgrade(&upgrade.request);
    upgrade.reject(
        "HTTP/1.1 401 Unauthorized",
        r#"{"error":"invalid_auth_token"}"#,
    );
    std::fs::write(race_directory.join("stream-reject-1"), "rejected").unwrap();

    wait_for_marker(stop, race_directory, "stream-retry-403");
    let upgrade = next_stream(listener, stop, deadline);
    assert_printer_events_upgrade(&upgrade.request);
    upgrade.reject(
        "HTTP/1.1 403 Forbidden",
        r#"{"error":"raw-forbidden-message","token":"secret"}"#,
    );
    std::fs::write(race_directory.join("stream-reject-2"), "rejected").unwrap();
}

fn wait_for_marker(stop: &AtomicBool, directory: &Path, name: &str) {
    let marker = directory.join(name);
    while !marker.exists() {
        if stop.load(Ordering::Acquire) {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
}

pub(super) fn serve_account_transition(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    _race_directory: &Path,
) {
    for _ in 0..2 {
        let upgrade = next_stream(listener, stop, deadline);
        assert!(
            upgrade
                .request
                .contains("/printer-events?projection=studio&version=1")
        );
        let frames = upgrade.serve();
        for frame in snapshot_frames(&filament_switch_printers_response()) {
            frames
                .send(frame)
                .expect("serve account transition snapshot");
        }
    }
}

pub(super) fn serve_token_rotation(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    race_directory: &Path,
    _offline_retry: bool,
) {
    let mut stale_rejected = false;
    let mut rotated_served = false;
    while !rotated_served {
        let upgrade = next_stream(listener, stop, deadline);
        if upgrade
            .request
            .contains("authorization: Bearer stale-token")
        {
            if stale_rejected {
                continue;
            }
            stale_rejected = true;
            upgrade.reject("HTTP/1.1 401 Unauthorized", r#"{"error":"token_expired"}"#);
        } else if upgrade
            .request
            .contains("authorization: Bearer probe-token")
        {
            let frames = upgrade.serve();
            for frame in snapshot_frames(&filament_switch_printers_response()) {
                frames.send(frame).expect("serve probe-token snapshot");
            }
        } else {
            assert!(
                upgrade
                    .request
                    .contains("authorization: Bearer rotated-token")
            );
            let frames = upgrade.serve();
            for frame in snapshot_frames(&filament_switch_printers_response()) {
                frames.send(frame).expect("serve rotated snapshot");
            }
            std::fs::write(race_directory.join("rotation-rotated-served"), "served").unwrap();
            let arm = race_directory.join("rotation-invalid-arm");
            thread::spawn(move || {
                while !arm.exists() {
                    thread::sleep(Duration::from_millis(5));
                }
                frames
                    .send(
                        r#"{"type":"printer_removed","dev_id":"studio-serial-1","pandar_printer_id":"printer-1"}"#
                            .to_owned(),
                    )
                    .expect("arm invalid rotated status");
            });
            rotated_served = true;
        }
    }

    while !stop.load(Ordering::Acquire) {
        match next_incoming(listener, stop, deadline) {
            Incoming::Stream(upgrade) => {
                let frames = upgrade.serve();
                for frame in snapshot_frames(&filament_switch_printers_response()) {
                    frames
                        .send(frame)
                        .expect("serve trailing rotation snapshot");
                }
            }
            Incoming::Http(mut stream, request) => {
                if request.lines().next() == Some("GET /healthz HTTP/1.1") {
                    write_response(
                        &mut stream,
                        "HTTP/1.1 200 OK",
                        r#"{"status":"ok","checks":{}}"#,
                    );
                } else if !super::firmware_compat::try_respond(&mut stream, &request) {
                    panic!("unexpected token rotation request: {request}");
                }
            }
        }
    }
}
