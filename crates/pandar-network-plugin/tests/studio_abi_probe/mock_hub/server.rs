use std::{
    net::TcpListener,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
    thread,
    time::Duration,
};

use crate::support::{assert_multipart_file_part, assert_multipart_print_request, request_body};

use super::{
    Incoming, MockHub, MockMode,
    account_race::serve_account_exchange_race,
    admission::serve_request_admission,
    connection::{
        serve_account_transition, serve_auth_rejected, serve_background_timeout,
        serve_connection_readiness, serve_no_auth_recovery, serve_stream_unavailable,
        serve_token_rotation,
    },
    freshness::{serve_firmware_claim_race, serve_freshness_claim},
    native, next_stream,
    operations::{TestOperation, assert_operation_body_eq},
    presence::{serve_axis_features, serve_callback_order, serve_printer_presence},
    responses::{
        PRINTERS_RESPONSE, camera_printers_response, filament_switch_printers_response,
        snapshot_frames,
    },
    synchronization::start_gate,
    transport::{assert_request_with_token, write_response},
};

pub(super) fn spawn(mode: MockMode, artifact: Vec<u8>, race_directory: &Path) -> MockHub {
    if matches!(
        mode,
        MockMode::NoAuthRecovery
            | MockMode::OfficialNoAuthRecovery
            | MockMode::OfficialNoAuthLogoutRecovery
    ) {
        return spawn_no_auth_recovery(
            race_directory,
            matches!(
                mode,
                MockMode::OfficialNoAuthRecovery | MockMode::OfficialNoAuthLogoutRecovery
            ),
            matches!(mode, MockMode::OfficialNoAuthLogoutRecovery),
        );
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let race_directory = race_directory.to_owned();
    let (start, started) = start_gate();
    let handle = thread::spawn(move || {
        let deadline = started.wait();
        match mode {
            MockMode::Success => serve_success(&listener, &thread_stop, deadline, &artifact),
            MockMode::ConnectionReadiness => {
                serve_connection_readiness(&listener, &thread_stop, deadline, &race_directory);
            }
            MockMode::BackgroundTimeout => {
                serve_background_timeout(&listener, &thread_stop, deadline, &race_directory);
            }
            MockMode::StreamUnavailable => {
                serve_stream_unavailable(&listener, &thread_stop, deadline, &race_directory);
            }
            MockMode::AuthRejected => {
                serve_auth_rejected(&listener, &thread_stop, deadline, &race_directory);
            }
            MockMode::PrinterPresence => {
                serve_printer_presence(&listener, &thread_stop, deadline, &race_directory);
            }
            MockMode::AccountTransition => {
                serve_account_transition(&listener, &thread_stop, deadline, &race_directory);
            }
            MockMode::AccountExchangeRace => {
                serve_account_exchange_race(&listener, &thread_stop, deadline, &race_directory);
            }
            MockMode::TokenRotation => {
                serve_token_rotation(&listener, &thread_stop, deadline, &race_directory, false);
            }
            MockMode::TokenRotationOffline => {
                serve_token_rotation(&listener, &thread_stop, deadline, &race_directory, true);
            }
            MockMode::FreshnessClaim => {
                serve_freshness_claim(&listener, &thread_stop, deadline, &race_directory);
            }
            MockMode::FirmwareClaimRace => {
                serve_firmware_claim_race(&listener, &thread_stop, deadline, &race_directory);
            }
            MockMode::CallbackOrder => {
                serve_callback_order(&listener, &thread_stop, deadline, &race_directory);
            }
            MockMode::RequestAdmission => {
                serve_request_admission(&listener, &thread_stop, deadline, &race_directory);
            }
            MockMode::CameraAvailable => {
                let upgrade = next_stream(&listener, &thread_stop, deadline);
                assert_printer_events_upgrade(&upgrade.request);
                let frames = upgrade.serve();
                for frame in snapshot_frames(&camera_printers_response()) {
                    frames.send(frame).expect("serve camera snapshot");
                }
            }
            MockMode::CameraUnavailable => {
                let upgrade = next_stream(&listener, &thread_stop, deadline);
                assert_printer_events_upgrade(&upgrade.request);
                let frames = upgrade.serve();
                for frame in snapshot_frames(&filament_switch_printers_response()) {
                    frames.send(frame).expect("serve camera snapshot");
                }
            }
            MockMode::NoAuthRecovery => unreachable!(),
            MockMode::OfficialNoAuthRecovery => unreachable!(),
            MockMode::OfficialNoAuthLogoutRecovery => unreachable!(),
            MockMode::ServerSelectionRestore => {
                serve_server_selection_restore(&listener, &thread_stop, deadline)
            }
            MockMode::Failure => serve_failure(&listener, &thread_stop, deadline),
            MockMode::NativePrintError => native::serve(&listener, &thread_stop, deadline),
            MockMode::AxisFeatures => serve_axis_features(&listener, &thread_stop, deadline),
        }
    });
    MockHub {
        url,
        handle,
        stop,
        start,
    }
}

fn spawn_no_auth_recovery(
    race_directory: &Path,
    zero_touch: bool,
    allow_session_delete: bool,
) -> MockHub {
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reservation.local_addr().unwrap();
    drop(reservation);
    let url = format!("http://{address}");
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let race_directory = race_directory.to_owned();
    let (start, started) = start_gate();
    let handle = thread::spawn(move || {
        let deadline = started.wait();
        let initial_failure = race_directory.join("no-auth-initial-failure-observed");
        while !initial_failure.exists() && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            initial_failure.exists(),
            "Studio ABI probe did not observe the initial no-auth connect failure"
        );
        let listener = TcpListener::bind(address).unwrap();
        listener.set_nonblocking(true).unwrap();
        serve_no_auth_recovery(
            &listener,
            &thread_stop,
            deadline,
            &race_directory,
            zero_touch,
            allow_session_delete,
        );
    });
    MockHub {
        url,
        handle,
        stop,
        start,
    }
}

fn serve_success(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: std::time::Instant,
    artifact: &[u8],
) {
    let steps: &[(&str, &str, Option<&str>)] = &[
        ("POST", "/api/v1/plugin/no-auth-session", None),
        ("POST", "/api/v1/plugin/login-tickets/exchange", None),
        ("POST", "/api/v1/plugin/login-tickets/exchange", None),
        (
            "GET",
            "/api/v1/plugin/jobs?dev_id=studio-serial-1&status=0&offset=0&limit=20",
            Some("probe-token"),
        ),
        ("POST", "/api/v1/plugin/prints", Some("probe-token")),
        ("GET", "/api/v1/plugin/jobs/38191", Some("probe-token")),
        ("GET", "/api/v1/plugin/jobs/38191", Some("probe-token")),
        (
            "POST",
            "/api/v1/plugin/printers/printer-1/operations",
            Some("probe-token"),
        ),
        (
            "POST",
            "/api/v1/plugin/printers/printer-1/operations",
            Some("probe-token"),
        ),
        (
            "POST",
            "/api/v1/plugin/printers/printer-1/operations",
            Some("probe-token"),
        ),
    ];
    let mut step = 0;
    while step < steps.len() {
        match super::next_incoming(listener, stop, deadline) {
            Incoming::Stream(upgrade) => {
                assert_printer_events_upgrade(&upgrade.request);
                let snapshot = snapshot_frames(&filament_switch_printers_response());
                let frames = upgrade.serve();
                for frame in snapshot {
                    frames.send(frame).expect("serve stream snapshot");
                }
            }
            Incoming::Http(mut stream, request) => {
                let (method, path, bearer) = steps[step];
                assert_request_with_token(&request, method, path, bearer);
                match step {
                    0 => write_response(
                        &mut stream,
                        "HTTP/1.1 403 Forbidden",
                        r#"{"error":"no_auth_required"}"#,
                    ),
                    1 | 2 => write_response(
                        &mut stream,
                        "HTTP/1.1 200 OK",
                        r#"{"token":"probe-token","profile":{"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"}}"#,
                    ),
                    3 => write_response(&mut stream, "HTTP/1.1 200 OK", r#"{"total":0,"hits":[]}"#),
                    4 => respond_to_print(&mut stream, &request, artifact),
                    5 => write_response(
                        &mut stream,
                        "HTTP/1.1 200 OK",
                        r#"{"studio_submission_id":38191,"job_status":"acknowledged","print_status":"pending"}"#,
                    ),
                    6 => write_response(
                        &mut stream,
                        "HTTP/1.1 200 OK",
                        r#"{"studio_submission_id":38191,"job_status":"succeeded","print_status":"pending"}"#,
                    ),
                    7 => respond_to_operation(
                        &mut stream,
                        &request,
                        TestOperation::SetChamberLight { light_on: false },
                        "cmd-light",
                    ),
                    8 => respond_to_operation(
                        &mut stream,
                        &request,
                        TestOperation::SetHotendTemperature {
                            temperature_celsius: 245,
                            wait: false,
                            extruder_id: 1,
                        },
                        "cmd-hotend",
                    ),
                    9 => {
                        assert_operation_body_eq(
                            &request,
                            TestOperation::Home {
                                axes: vec!["x".to_owned()],
                            },
                        );
                        assert!(!request_body(&request).contains("G28"));
                        write_response(
                            &mut stream,
                            "HTTP/1.1 202 Accepted",
                            r#"{"command_id":"cmd-1","status":"queued"}"#,
                        );
                    }
                    _ => unreachable!(),
                }
                step += 1;
            }
        }
    }
}

/// Validates the Studio-projection printer-events upgrade contract.
pub(super) fn assert_printer_events_upgrade_for_tenant(request: &str, tenant: &str) {
    let expected = format!(
        "GET /api/v1/tenants/{tenant}/printer-events?projection=studio&version=1 HTTP/1.1\r\n"
    );
    assert!(
        request.starts_with(&expected),
        "unexpected printer-events upgrade request line: {request}"
    );
    assert!(
        request.contains("authorization: Bearer probe-token\r\n"),
        "missing bearer auth on printer-events upgrade: {request}"
    );
}

pub(super) fn assert_printer_events_upgrade(request: &str) {
    assert_printer_events_upgrade_for_tenant(request, "tenant-1");
}

fn respond_to_print(stream: &mut std::net::TcpStream, request: &str, artifact: &[u8]) {
    let body = request_body(request);
    assert_multipart_print_request(request);
    assert!(body.contains(r#"name="printer_id""#));
    assert!(body.contains("printer-1") && !body.contains("studio-serial-1"));
    for field in [
        "nozzle_mapping",
        "ams_mapping",
        "ams_mapping2",
        "ams_mapping_info",
        "nozzles_info",
    ] {
        assert!(body.contains(&format!(r#"name="{field}""#)));
    }
    assert!(!body.contains("\r\nnull\r\n"));
    assert!(body.contains(r#"name="filename""#));
    assert_multipart_file_part(request, "probe.3mf", artifact);
    write_response(
        stream,
        "HTTP/1.1 201 Created",
        r#"{"task_id":38191,"studio_submission_id":38191,"status":"queued"}"#,
    );
}

fn respond_to_operation(
    stream: &mut std::net::TcpStream,
    request: &str,
    operation: TestOperation,
    command_id: &str,
) {
    assert_operation_body_eq(request, operation);
    write_response(
        stream,
        "HTTP/1.1 202 Accepted",
        &format!(r#"{{"command_id":"{command_id}","status":"queued"}}"#),
    );
}

/// Serves the server-selection restore probe: the initial no-auth attempt is refused
/// (the user must sign in), and every ticket exchange succeeds with the probe token.
fn serve_server_selection_restore(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: std::time::Instant,
) {
    loop {
        match super::next_incoming(listener, stop, deadline) {
            Incoming::Stream(upgrade) => {
                assert_printer_events_upgrade(&upgrade.request);
                let frames = upgrade.serve();
                for frame in snapshot_frames(&filament_switch_printers_response()) {
                    frames.send(frame).expect("serve server-selection snapshot");
                }
            }
            Incoming::Http(mut stream, request) => {
                let line = request.lines().next().unwrap_or_default();
                match line {
                    "POST /api/v1/plugin/no-auth-session HTTP/1.1" => write_response(
                        &mut stream,
                        "HTTP/1.1 403 Forbidden",
                        r#"{"error":"no_auth_required"}"#,
                    ),
                    "POST /api/v1/plugin/login-tickets/exchange HTTP/1.1" => write_response(
                        &mut stream,
                        "HTTP/1.1 200 OK",
                        r#"{"token":"probe-token","profile":{"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"}}"#,
                    ),
                    _ => panic!("unexpected server-selection restore request: {line}"),
                }
            }
        }
    }
}

fn serve_failure(listener: &TcpListener, stop: &AtomicBool, deadline: std::time::Instant) {
    let steps: &[(&str, &str, Option<&str>)] = &[
        ("POST", "/api/v1/plugin/no-auth-session", None),
        ("POST", "/api/v1/plugin/login-tickets/exchange", None),
        ("POST", "/api/v1/plugin/prints", Some("probe-token")),
    ];
    let mut step = 0;
    let mut stream_rejections = 0;
    while step < steps.len() {
        match super::next_incoming(listener, stop, deadline) {
            Incoming::Stream(upgrade) => {
                assert_printer_events_upgrade_for_tenant(&upgrade.request, "tenant");
                if stream_rejections < 1 {
                    stream_rejections += 1;
                    upgrade.reject(
                        "HTTP/1.1 401 Unauthorized",
                        r#"{"error":"raw-auth-message","token":"secret"}"#,
                    );
                } else {
                    let frames = upgrade.serve();
                    for frame in snapshot_frames(PRINTERS_RESPONSE) {
                        frames.send(frame).expect("serve failure-mode snapshot");
                    }
                }
            }
            Incoming::Http(mut stream, request) => {
                let (method, path, bearer) = steps[step];
                assert_request_with_token(&request, method, path, bearer);
                match step {
                    0 => write_response(
                        &mut stream,
                        "HTTP/1.1 403 Forbidden",
                        r#"{"error":"no_auth_required"}"#,
                    ),
                    1 => write_response(
                        &mut stream,
                        "HTTP/1.1 401 Unauthorized",
                        r#"{"error":"raw-ticket-message","ticket":"secret"}"#,
                    ),
                    2 => write_response(
                        &mut stream,
                        "HTTP/1.1 403 Forbidden",
                        r#"{"error":"raw-forbidden-message","path":"/tmp/secret.3mf"}"#,
                    ),
                    _ => unreachable!(),
                }
                step += 1;
            }
        }
    }
}
