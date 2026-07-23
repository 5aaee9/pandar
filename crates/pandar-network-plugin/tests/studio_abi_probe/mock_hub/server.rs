use std::{
    net::TcpListener,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
    thread,
    time::Duration,
};

use crate::support::{assert_multipart_file_part, assert_multipart_print_request, request_body};

use super::{
    MockHub, MockMode,
    account_race::serve_account_exchange_race,
    admission::serve_request_admission,
    connection::{
        serve_account_transition, serve_auth_rejected, serve_background_timeout,
        serve_connection_readiness, serve_no_auth_recovery, serve_token_rotation,
    },
    freshness::{serve_firmware_claim_race, serve_freshness_claim},
    native, next_request_allow_ready,
    operations::{TestOperation, assert_operation_body_eq},
    presence::{serve_axis_features, serve_callback_order, serve_printer_presence},
    responses::{
        PRINTERS_RESPONSE, filament_switch_printers_response, printers_response_with_progress,
    },
    synchronization::start_gate,
    transport::{assert_request, assert_request_with_token, write_response},
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
                serve_connection_readiness(&listener, &thread_stop, deadline);
            }
            MockMode::BackgroundTimeout => {
                serve_background_timeout(&listener, &thread_stop, deadline);
            }
            MockMode::AuthRejected => serve_auth_rejected(&listener, &thread_stop, deadline),
            MockMode::PrinterPresence => {
                serve_printer_presence(&listener, &thread_stop, deadline);
            }
            MockMode::AccountTransition => {
                serve_account_transition(&listener, &thread_stop, deadline);
            }
            MockMode::AccountExchangeRace => {
                serve_account_exchange_race(&listener, &thread_stop, deadline, &race_directory);
            }
            MockMode::TokenRotation => {
                serve_token_rotation(&listener, &thread_stop, deadline, false);
            }
            MockMode::TokenRotationOffline => {
                serve_token_rotation(&listener, &thread_stop, deadline, true);
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
                serve_request_admission(&listener, &thread_stop, deadline);
            }
            MockMode::CameraUnavailable => {
                let (mut stream, request) = next_request_allow_ready(
                    &listener,
                    &thread_stop,
                    deadline,
                    "GET",
                    "/api/v1/plugin/printers",
                );
                assert_request_with_token(
                    &request,
                    "GET",
                    "/api/v1/plugin/printers",
                    Some("probe-token"),
                );
                write_response(
                    &mut stream,
                    "HTTP/1.1 200 OK",
                    &filament_switch_printers_response(),
                );
            }
            MockMode::NoAuthRecovery => unreachable!(),
            MockMode::OfficialNoAuthRecovery => unreachable!(),
            MockMode::OfficialNoAuthLogoutRecovery => unreachable!(),
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
    let expected = [
        ("POST", "/api/v1/plugin/no-auth-session", false),
        ("POST", "/api/v1/plugin/login-tickets/exchange", false),
        ("POST", "/api/v1/plugin/login-tickets/exchange", false),
        ("GET", "/api/v1/plugin/printers", true),
        ("GET", "/api/v1/plugin/printers", true),
        (
            "GET",
            "/api/v1/plugin/jobs?dev_id=studio-serial-1&status=0&offset=0&limit=20",
            true,
        ),
        ("POST", "/api/v1/plugin/prints", true),
        ("GET", "/api/v1/plugin/jobs/38191", true),
        ("GET", "/api/v1/plugin/jobs/38191", true),
        ("GET", "/api/v1/plugin/printers", true),
        ("POST", "/api/v1/plugin/printers/printer-1/operations", true),
        ("POST", "/api/v1/plugin/printers/printer-1/operations", true),
        ("GET", "/api/v1/plugin/printers", true),
        ("POST", "/api/v1/plugin/printers/printer-1/operations", true),
        ("GET", "/api/v1/plugin/printers", true),
    ];
    for (index, (method, path, bearer)) in expected.into_iter().enumerate() {
        let (mut stream, request) =
            next_request_allow_ready(listener, stop, deadline, method, path);
        assert_request(&request, method, path, bearer);
        match index {
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
            3 => write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                &printers_response_with_progress(36),
            ),
            4 | 9 | 12 | 14 => write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                &filament_switch_printers_response(),
            ),
            5 => write_response(&mut stream, "HTTP/1.1 200 OK", r#"{"total":0,"hits":[]}"#),
            6 => respond_to_print(&mut stream, &request, artifact),
            7 => write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                r#"{"studio_submission_id":38191,"job_status":"acknowledged","print_status":"pending"}"#,
            ),
            8 => write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                r#"{"studio_submission_id":38191,"job_status":"succeeded","print_status":"pending"}"#,
            ),
            10 => respond_to_operation(
                &mut stream,
                &request,
                TestOperation::SetChamberLight { light_on: false },
                "cmd-light",
            ),
            11 => respond_to_operation(
                &mut stream,
                &request,
                TestOperation::SetHotendTemperature {
                    temperature_celsius: 245,
                    wait: false,
                    extruder_id: 1,
                },
                "cmd-hotend",
            ),
            13 => {
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
    }
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

fn serve_failure(listener: &TcpListener, stop: &AtomicBool, deadline: std::time::Instant) {
    let expected = [
        ("POST", "/api/v1/plugin/no-auth-session", false),
        ("POST", "/api/v1/plugin/login-tickets/exchange", false),
        ("GET", "/api/v1/plugin/printers", true),
        ("GET", "/api/v1/plugin/printers", true),
        ("POST", "/api/v1/plugin/prints", true),
    ];
    for (index, (method, path, bearer)) in expected.into_iter().enumerate() {
        let (mut stream, request) =
            next_request_allow_ready(listener, stop, deadline, method, path);
        assert_request(&request, method, path, bearer);
        let (status, body) = match index {
            0 => ("HTTP/1.1 403 Forbidden", r#"{"error":"no_auth_required"}"#),
            1 => (
                "HTTP/1.1 401 Unauthorized",
                r#"{"error":"raw-ticket-message","ticket":"secret"}"#,
            ),
            2 => (
                "HTTP/1.1 401 Unauthorized",
                r#"{"error":"raw-auth-message","token":"secret"}"#,
            ),
            3 => ("HTTP/1.1 200 OK", PRINTERS_RESPONSE),
            4 => (
                "HTTP/1.1 403 Forbidden",
                r#"{"error":"raw-forbidden-message","path":"/tmp/secret.3mf"}"#,
            ),
            _ => unreachable!(),
        };
        write_response(&mut stream, status, body);
    }
}
