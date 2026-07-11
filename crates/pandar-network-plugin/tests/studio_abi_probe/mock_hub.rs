#[path = "mock_hub/native.rs"]
mod native;
#[path = "mock_hub/operations.rs"]
mod operations;
#[path = "mock_hub/synchronization.rs"]
mod synchronization;
#[path = "mock_hub/transport.rs"]
mod transport;

use crate::support::{assert_multipart_file_part, assert_multipart_print_request, request_body};
pub(super) use operations::required_device_feature_presence_matches;
use operations::{
    AxisFeatureOperation, TestOperation, assert_axis_feature_operation_body_eq,
    assert_operation_body_eq,
};
use std::{
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Instant,
};
use synchronization::{ProbeStart, start_gate};
use transport::{assert_request, assert_request_with_token, read_request_until, write_response};

const PRINTERS_RESPONSE: &str = r##"{"message":"success","devices":[{"dev_id":"studio-serial-1","fun":"8000004100000020","dev_name":"Probe Printer","pandar_printer_id":"printer-1","name":"Probe Printer","dev_ip":"192.0.2.10","dev_access_code":"12345678","dev_model_name":"N6","model":"N6","dev_online":true,"online":true,"task_status":"RUNNING","state":"RUNNING","gcode_state":"RUNNING","mc_percent":37,"mc_remaining_time":52,"layer_num":12,"total_layer_num":120,"task_id":"task-42","subtask_id":"subtask-42","gcode_file":"drawer-organizer.gcode","subtask_name":"drawer-organizer","hms":[{"attr":134152704,"code":32785}],"nozzle_temperatures":[{"label":"L","current_celsius":"28","target_celsius":"220","diameter_mm":"0.4","nozzle_type":"HH05"},{"label":"R","current_celsius":"27","target_celsius":"215","diameter_mm":"0.4","nozzle_type":"HS01"}],"active_nozzle":"L","bed_temperature_celsius":"60","bed_target_temperature_celsius":"65","chamber_temperature_celsius":"32","chamber_light_on":true,"materials":{"ams_units":[{"unit_id":"0","humidity":25,"humidity_level":3,"temperature_celsius":28.5,"toolhead":"R","trays":[{"tray_id":"0","global_tray_id":0,"type":"PETG-CF","filament_id":"GFG50","color":"000000FF","remaining_estimate":"-1"},{"tray_id":"1","global_tray_id":1,"type":"PLA","filament_id":"GFA00","color":"C12E1FFF","remaining_estimate":"100"},{"tray_id":"2","global_tray_id":2,"type":"PETG","filament_id":"GFG00","color":"FCE300FF","remaining_estimate":"36"},{"tray_id":"3","global_tray_id":3,"type":"PLA","filament_id":"GFL99","color":"FFF144FF","remaining_estimate":"-1"}]},{"unit_id":"1","humidity":28,"humidity_level":3,"temperature_celsius":28.1,"toolhead":"L","trays":[{"tray_id":"0","global_tray_id":4,"type":"PLA","filament_id":"GFA00","color":"000000FF","remaining_estimate":"55"},{"tray_id":"1","global_tray_id":5,"type":"ABS","filament_id":"GFB00","color":"46A8F9FF","remaining_estimate":"-1"},{"tray_id":"2","global_tray_id":6,"type":"ABS","filament_id":"GFB00","color":"057748FF","remaining_estimate":"-1"},{"tray_id":"3","global_tray_id":7,"type":"PLA-CF","filament_id":"GFA50","color":"69398EFF","remaining_estimate":"85"}]}],"external_spools":[{"external_id":"254","tray_id":"0","type":"PETG","filament_id":"GFG00","color":"11223344","toolhead":"L"},{"external_id":"255","tray_id":"1","type":"PLA","filament_id":"GFL99","color":"46A8F9FF","toolhead":"R"}],"active_tray":{"kind":"ams","ams_id":"0","tray_id":"3","global_tray_id":3},"observed_at":"2026-06-20T00:01:00Z"}}]}"##;

fn printers_response_with_progress(progress: u8) -> String {
    PRINTERS_RESPONSE.replacen(
        r#""mc_percent":37"#,
        &format!(r#""mc_percent":{progress}"#),
        1,
    )
}

fn next_request(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
    method: &str,
    path: &str,
) -> (TcpStream, String) {
    let waiting_for = format!("{method} {path}");
    read_request_until(listener, stop, deadline, &waiting_for)
        .unwrap_or_else(|| panic!("Studio ABI probe exited before {waiting_for}"))
}

#[derive(Clone, Copy)]
pub(super) enum MockMode {
    Success,
    Failure,
    StaleTokenRefresh,
    NativePrintError,
    AxisFeatures,
}

pub(super) struct MockHub {
    pub(super) url: String,
    handle: thread::JoinHandle<()>,
    stop: Arc<AtomicBool>,
    start: ProbeStart,
}

impl MockHub {
    pub(super) fn start(&self, deadline: Instant) {
        self.start.arm(deadline);
    }

    pub(super) fn finish(self) -> thread::Result<()> {
        self.stop.store(true, Ordering::Release);
        self.handle.join()
    }
}

fn serve_axis_features(listener: &TcpListener, stop: &AtomicBool, deadline: Instant) {
    let mut operation_posts = 0_u32;
    while !stop.load(Ordering::Acquire) {
        let Some((mut stream, request)) =
            read_request_until(listener, stop, deadline, "axis feature probe request")
        else {
            return;
        };
        let line = request.lines().next().unwrap_or_default();
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
            write_response(&mut stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE);
        } else if line == "GET /probe-operation-count HTTP/1.1" {
            write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                &serde_json::json!({"count": operation_posts}).to_string(),
            );
        } else if line == "POST /api/v1/plugin/printers/printer-1/operations HTTP/1.1" {
            let expected = match operation_posts {
                0 | 4 => AxisFeatureOperation::modern_home(),
                1 => AxisFeatureOperation::modern_move("x", 1.0),
                2 | 6 => AxisFeatureOperation::legacy_home(),
                3 => AxisFeatureOperation::legacy_move("x", 10.0, 3000),
                5 => AxisFeatureOperation::modern_move("z", -10.0),
                7 => AxisFeatureOperation::legacy_move("z", -1.0, 600),
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

pub(super) fn spawn_mock_hub(mode: MockMode, artifact: Vec<u8>) -> MockHub {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());

    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let (start, started) = start_gate();
    let handle = thread::spawn(move || {
        let deadline = started.wait();
        match mode {
            MockMode::Success => {
                let expected = [
                    ("POST", "/api/v1/plugin/no-auth-session", false),
                    ("POST", "/api/v1/plugin/login-tickets/exchange", false),
                    ("POST", "/api/v1/plugin/login-tickets/exchange", false),
                    ("GET", "/api/v1/plugin/printers", true),
                    ("GET", "/api/v1/plugin/printers", true),
                    ("POST", "/api/v1/plugin/prints", true),
                    ("GET", "/api/v1/plugin/printers", true),
                    ("GET", "/api/v1/plugin/printers", true),
                    ("POST", "/api/v1/plugin/printers/printer-1/operations", true),
                    ("POST", "/api/v1/plugin/printers/printer-1/operations", true),
                    ("GET", "/api/v1/plugin/printers", true),
                    ("POST", "/api/v1/plugin/printers/printer-1/operations", true),
                    ("GET", "/api/v1/plugin/printers", true),
                ];
                for (index, (method, path, bearer)) in expected.into_iter().enumerate() {
                    let (mut stream, request) =
                        next_request(&listener, &thread_stop, deadline, method, path);
                    assert_request(&request, method, path, bearer);
                    match index {
                        0 => write_response(
                            &mut stream,
                            "HTTP/1.1 403 Forbidden",
                            r#"{"error":"no_auth_required"}"#,
                        ),
                        1 => write_response(
                            &mut stream,
                            "HTTP/1.1 200 OK",
                            r#"{"token":"probe-token","profile":{"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"}}"#,
                        ),
                        2 => write_response(
                            &mut stream,
                            "HTTP/1.1 200 OK",
                            r#"{"token":"probe-token","profile":{"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"}}"#,
                        ),
                        3 => write_response(
                            &mut stream,
                            "HTTP/1.1 200 OK",
                            &printers_response_with_progress(36),
                        ),
                        4 => write_response(&mut stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE),
                        5 => {
                            let body = request_body(&request);
                            assert_multipart_print_request(&request);
                            assert!(
                                body.contains(r#"name="printer_id""#),
                                "bad print body: {body}"
                            );
                            assert!(
                                body.contains("printer-1") && !body.contains("studio-serial-1"),
                                "print body did not use Hub printer id: {body}"
                            );
                            assert!(
                                !body.contains(r#"name="ams_mapping""#)
                                    && !body.contains(r#"name="ams_mapping2""#)
                                    && !body.contains("\r\nnull\r\n"),
                                "empty print mappings should be omitted: {body}"
                            );
                            assert!(
                                body.contains(r#"name="filename""#),
                                "bad print filename: {body}"
                            );
                            assert_multipart_file_part(&request, "probe.3mf", &artifact);
                            write_response(&mut stream, "HTTP/1.1 200 OK", r#"{"job_id":"job-1"}"#);
                        }
                        6 | 7 => write_response(&mut stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE),
                        8 => {
                            assert_operation_body_eq(
                                &request,
                                TestOperation::SetChamberLight { light_on: false },
                            );
                            write_response(
                                &mut stream,
                                "HTTP/1.1 202 Accepted",
                                r#"{"command_id":"cmd-light","status":"queued"}"#,
                            );
                        }
                        9 => {
                            assert_operation_body_eq(
                                &request,
                                TestOperation::SetHotendTemperature {
                                    temperature_celsius: 245,
                                    wait: false,
                                    extruder_id: 1,
                                },
                            );
                            write_response(
                                &mut stream,
                                "HTTP/1.1 202 Accepted",
                                r#"{"command_id":"cmd-hotend","status":"queued"}"#,
                            );
                        }
                        10 => write_response(&mut stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE),
                        11 => {
                            assert_operation_body_eq(
                                &request,
                                TestOperation::Home {
                                    axes: vec!["x".to_owned()],
                                },
                            );
                            assert!(
                                !request_body(&request).contains("G28"),
                                "operation request leaked raw G-code: {request}"
                            );
                            write_response(
                                &mut stream,
                                "HTTP/1.1 202 Accepted",
                                r#"{"command_id":"cmd-1","status":"queued"}"#,
                            );
                        }
                        12 => write_response(&mut stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE),
                        _ => unreachable!(),
                    }
                }
            }
            MockMode::StaleTokenRefresh => {
                let expected = [
                    ("GET", "/api/v1/plugin/printers", Some("stale-token")),
                    ("POST", "/api/v1/plugin/no-auth-session", None),
                    ("GET", "/api/v1/plugin/printers", Some("probe-token")),
                    ("GET", "/api/v1/plugin/printers", Some("probe-token")),
                    ("POST", "/api/v1/plugin/prints", Some("probe-token")),
                    ("GET", "/api/v1/plugin/printers", Some("probe-token")),
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
                    ("GET", "/api/v1/plugin/printers", Some("probe-token")),
                    (
                        "POST",
                        "/api/v1/plugin/printers/printer-1/operations",
                        Some("probe-token"),
                    ),
                    ("GET", "/api/v1/plugin/printers", Some("probe-token")),
                ];
                for (index, (method, path, bearer_token)) in expected.into_iter().enumerate() {
                    let (mut stream, request) =
                        next_request(&listener, &thread_stop, deadline, method, path);
                    assert_request_with_token(&request, method, path, bearer_token);
                    match index {
                        0 => write_response(
                            &mut stream,
                            "HTTP/1.1 401 Unauthorized",
                            r#"{"error":"token_expired"}"#,
                        ),
                        1 => write_response(
                            &mut stream,
                            "HTTP/1.1 200 OK",
                            r#"{"token":"probe-token","profile":{"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"}}"#,
                        ),
                        2 | 3 => write_response(&mut stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE),
                        4 => {
                            let body = request_body(&request);
                            assert_multipart_print_request(&request);
                            assert!(
                                body.contains("printer-1") && !body.contains("studio-serial-1"),
                                "print body did not use Hub printer id: {body}"
                            );
                            assert!(
                                !body.contains(r#"name="ams_mapping""#)
                                    && !body.contains(r#"name="ams_mapping2""#)
                                    && !body.contains("\r\nnull\r\n"),
                                "empty print mappings should be omitted: {body}"
                            );
                            assert!(body.contains("probe.3mf"), "bad print filename: {body}");
                            assert_multipart_file_part(&request, "probe.3mf", &artifact);
                            write_response(&mut stream, "HTTP/1.1 200 OK", r#"{"job_id":"job-1"}"#);
                        }
                        5 => write_response(
                            &mut stream,
                            "HTTP/1.1 200 OK",
                            &PRINTERS_RESPONSE.replacen(
                                r#""mc_percent":37"#,
                                r#""mc_percent":99,"print_error":"83918929","job_id":42"#,
                                1,
                            ),
                        ),
                        6 => {
                            assert_operation_body_eq(
                                &request,
                                TestOperation::SetChamberLight { light_on: false },
                            );
                            write_response(
                                &mut stream,
                                "HTTP/1.1 202 Accepted",
                                r#"{"command_id":"cmd-light","status":"queued"}"#,
                            );
                        }
                        7 => {
                            assert_operation_body_eq(
                                &request,
                                TestOperation::SetHotendTemperature {
                                    temperature_celsius: 245,
                                    wait: false,
                                    extruder_id: 1,
                                },
                            );
                            write_response(
                                &mut stream,
                                "HTTP/1.1 202 Accepted",
                                r#"{"command_id":"cmd-hotend","status":"queued"}"#,
                            );
                        }
                        8 => write_response(&mut stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE),
                        9 => {
                            assert_operation_body_eq(
                                &request,
                                TestOperation::Home {
                                    axes: vec!["x".to_owned()],
                                },
                            );
                            assert!(
                                !request_body(&request).contains("G28"),
                                "operation request leaked raw G-code: {request}"
                            );
                            write_response(
                                &mut stream,
                                "HTTP/1.1 202 Accepted",
                                r#"{"command_id":"cmd-1","status":"queued"}"#,
                            );
                        }
                        10 => write_response(&mut stream, "HTTP/1.1 200 OK", PRINTERS_RESPONSE),
                        _ => unreachable!(),
                    }
                }
            }
            MockMode::Failure => {
                let expected = [
                    ("POST", "/api/v1/plugin/no-auth-session", false),
                    ("POST", "/api/v1/plugin/login-tickets/exchange", false),
                    ("GET", "/api/v1/plugin/printers", true),
                    ("POST", "/api/v1/plugin/no-auth-session", false),
                    ("GET", "/api/v1/plugin/printers", true),
                    ("POST", "/api/v1/plugin/prints", true),
                    ("GET", "/api/v1/plugin/printers", true),
                    ("GET", "/api/v1/plugin/printers", true),
                ];
                for (index, (method, path, bearer)) in expected.into_iter().enumerate() {
                    let (mut stream, request) =
                        next_request(&listener, &thread_stop, deadline, method, path);
                    assert_request(&request, method, path, bearer);
                    match index {
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
                            "HTTP/1.1 401 Unauthorized",
                            r#"{"error":"raw-auth-message","token":"secret"}"#,
                        ),
                        3 => write_response(
                            &mut stream,
                            "HTTP/1.1 200 OK",
                            r#"{"token":"probe-token","profile":{"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"}}"#,
                        ),
                        4 => write_response(
                            &mut stream,
                            "HTTP/1.1 401 Unauthorized",
                            r#"{"error":"raw-auth-message","token":"secret"}"#,
                        ),
                        5 => write_response(
                            &mut stream,
                            "HTTP/1.1 403 Forbidden",
                            r#"{"error":"raw-forbidden-message","path":"/tmp/secret.3mf"}"#,
                        ),
                        6 => write_response(
                            &mut stream,
                            "HTTP/1.1 503 Service Unavailable",
                            r#"{"error":"raw-refresh-message","token":"secret"}"#,
                        ),
                        7 => write_response(
                            &mut stream,
                            "HTTP/1.1 503 Service Unavailable",
                            r#"{"error":"raw-heartbeat-message","token":"secret"}"#,
                        ),
                        _ => unreachable!(),
                    }
                }
            }
            MockMode::NativePrintError => native::serve(&listener, &thread_stop, deadline),
            MockMode::AxisFeatures => {
                serve_axis_features(&listener, &thread_stop, deadline);
            }
        }
    });
    MockHub {
        url,
        handle,
        stop,
        start,
    }
}
