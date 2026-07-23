use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use crate::{harness::DIAGNOSTIC_SECRET, support::read_http_request_with_timeout};

pub(super) struct MockHub {
    pub(super) url: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    handle: thread::JoinHandle<()>,
}

impl MockHub {
    pub(super) fn spawn(case: &str, race_directory: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let thread_requests = Arc::clone(&requests);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let job_polls = Arc::new(AtomicUsize::new(0));
        let case = case.to_owned();
        let race_directory = race_directory.to_owned();
        let handle = thread::spawn(move || {
            while !thread_stop.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("mock Hub accept failed: {error}"),
                };
                stream.set_nonblocking(false).unwrap();
                let cancelled_upload =
                    case == "cancel_upload" && !thread_requests.lock().unwrap().is_empty();
                let request = if cancelled_upload {
                    read_cancelled_upload(&mut stream)
                } else {
                    read_http_request_with_timeout(&mut stream, Some(Duration::from_secs(2)))
                };
                thread_requests.lock().unwrap().push(request.clone());
                if cancelled_upload {
                    continue;
                }
                let first = request.lines().next().unwrap_or_default();
                hold_task_response(first, &case, &race_directory);
                let (status, body) = response_for(first, &job_polls, &case);
                if let Err(error) = write_response(&mut stream, status, &body) {
                    assert!(
                        matches!(
                            case.as_str(),
                            "model_task_destroy_inflight" | "model_task_destroy_no_auth_recovery"
                        ),
                        "mock Hub response failed: {error}"
                    );
                }
            }
        });
        Self {
            url,
            requests,
            stop,
            handle,
        }
    }

    pub(super) fn finish(self) -> Vec<String> {
        self.stop.store(true, Ordering::SeqCst);
        self.handle.join().expect("mock Hub thread");
        Arc::try_unwrap(self.requests)
            .expect("mock Hub request ownership")
            .into_inner()
            .unwrap()
    }
}

fn hold_task_response(first: &str, case: &str, race_directory: &Path) {
    if case == "model_task_destroy_no_auth_recovery"
        && first.starts_with("POST /api/v1/plugin/no-auth-session ")
    {
        std::fs::create_dir(race_directory.join("request-entered")).unwrap();
        thread::sleep(Duration::from_secs(3));
        return;
    }
    if case == "model_task_destroy_inflight"
        && first.starts_with("GET /api/v1/plugin/jobs/38191/model-task ")
    {
        std::fs::create_dir(race_directory.join("request-entered")).unwrap();
        thread::sleep(Duration::from_secs(3));
        return;
    }
    let should_hold = match case {
        "stale_task_list" => first.starts_with("GET /api/v1/plugin/jobs?"),
        "stale_task_plate" => first.starts_with("GET /api/v1/plugin/jobs/38191/plate "),
        "stale_task_subtask" => first.starts_with("GET /api/v1/plugin/jobs/38191/subtask "),
        "stale_model_task" => first.starts_with("GET /api/v1/plugin/jobs/38191/model-task "),
        "stale_during_detail" => first.starts_with("GET /api/v1/plugin/jobs/38191 "),
        _ => false,
    };
    if !should_hold {
        return;
    }
    std::fs::create_dir(race_directory.join("request-entered")).unwrap();
    let release = race_directory.join("release-request");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !release.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        release.exists(),
        "account freshness request was not released"
    );
}

fn read_cancelled_upload(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => request.extend_from_slice(&buffer[..read]),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                ) =>
            {
                break;
            }
            Err(error) => panic!("cancelled upload read failed: {error}"),
        }
    }
    String::from_utf8_lossy(&request).into_owned()
}

fn response_for(first: &str, job_polls: &AtomicUsize, case: &str) -> (&'static str, String) {
    if first.starts_with("GET /api/v1/plugin/printers ") {
        return ("HTTP/1.1 200 OK", printer_response().to_owned());
    }
    if first.starts_with("POST /api/v1/plugin/prints ") {
        return (
            "HTTP/1.1 201 Created",
            r#"{"task_id":38191,"studio_submission_id":38191,"status":"queued"}"#.to_owned(),
        );
    }
    if first.starts_with("POST /api/v1/plugin/jobs/38191/cancel ") {
        return cancel_response(case);
    }
    if first.starts_with("GET /api/v1/plugin/jobs/38191/model-task ") {
        return model_task_response(case);
    }
    if first.contains("/plate") {
        if case == "task_unknown" {
            return (
                "HTTP/1.1 404 Not Found",
                r#"{"error":"job_not_found"}"#.to_owned(),
            );
        }
        if case == "task_invalid_plate_2xx" {
            return (
                "HTTP/1.1 200 OK",
                r#"{"studio_submission_id":38191,"plate_index":0}"#.to_owned(),
            );
        }
        return (
            "HTTP/1.1 200 OK",
            r#"{"studio_submission_id":38191,"plate_index":7}"#.to_owned(),
        );
    }
    if first.contains("/subtask") {
        if case == "task_unknown" {
            return (
                "HTTP/1.1 404 Not Found",
                r#"{"error":"job_not_found"}"#.to_owned(),
            );
        }
        if case == "task_metadata_unavailable" {
            return (
                "HTTP/1.1 409 Conflict",
                r#"{"error":"studio_task_metadata_unavailable"}"#.to_owned(),
            );
        }
        if case == "task_invalid_subtask_2xx" {
            return (
                "HTTP/1.1 200 OK",
                r##"{"content":"{\"info\":{\"plate_idx\":7}}","context":{"plates":[{"index":7,"thumbnail":{"url":""},"prediction":3600,"weight":12.5,"filaments":[{"color":"","type":"PLA","used_g":"12.5","used_m":"4.2"}]}]}}"##.to_owned(),
            );
        }
        if case == "task_oversized_subtask_weight_2xx" {
            return (
                "HTTP/1.1 200 OK",
                r##"{"content":"{\"info\":{\"plate_idx\":7}}","context":{"plates":[{"index":7,"thumbnail":{"url":""},"prediction":3600,"weight":1.7976931348623157e308,"filaments":[{"color":"#112233","type":"PLA","used_g":"12.5","used_m":"4.2"}]}]}}"##.to_owned(),
            );
        }
        if case == "task_oversized_subtask_prediction_2xx" {
            return (
                "HTTP/1.1 200 OK",
                r##"{"content":"{\"info\":{\"plate_idx\":7}}","context":{"plates":[{"index":7,"thumbnail":{"url":""},"prediction":2147483648,"weight":12.5,"filaments":[{"color":"#112233","type":"PLA","used_g":"12.5","used_m":"4.2"}]}]}}"##.to_owned(),
            );
        }
        if case == "task_nonpositive_subtask_plate_2xx" {
            return (
                "HTTP/1.1 200 OK",
                r##"{"content":"{\"info\":{\"plate_idx\":0}}","context":{"plates":[{"index":0,"thumbnail":{"url":""},"prediction":3600,"weight":12.5,"filaments":[{"color":"#112233","type":"PLA","used_g":"12.5","used_m":"4.2"}]}]}}"##.to_owned(),
            );
        }
        if case == "task_mixed_invalid_subtask_2xx" {
            return (
                "HTTP/1.1 200 OK",
                r##"{"content":"{\"info\":{\"plate_idx\":7}}","context":{"plates":[{"index":7,"thumbnail":{"url":""},"prediction":3600,"weight":12.5,"filaments":[{"color":"#112233","type":"PLA","used_g":"12.5","used_m":"4.2"}]},{"index":-1,"thumbnail":{"url":"https://untrusted.invalid/private.png"},"prediction":2147483648,"weight":12.5,"filaments":[]}]}}"##.to_owned(),
            );
        }
        return ("HTTP/1.1 200 OK", subtask_response().to_owned());
    }
    if first.starts_with("GET /api/v1/plugin/jobs?")
        || first.starts_with("GET /api/v1/plugin/jobs ")
    {
        if case == "task_hub_outage" {
            return (
                "HTTP/1.1 503 Service Unavailable",
                r#"{"error":"hub_unavailable"}"#.to_owned(),
            );
        }
        if case == "task_sensitive_page_2xx" {
            return (
                "HTTP/1.1 200 OK",
                format!(r#"{{"total":"{DIAGNOSTIC_SECRET}","hits":[]}}"#),
            );
        }
        if case == "task_sensitive_error_4xx" {
            return (
                "HTTP/1.1 400 Bad Request",
                format!(r#"{{"error":{{"detail":"{DIAGNOSTIC_SECRET}"}}}}"#),
            );
        }
        if case == "task_oversized_total_2xx" {
            return (
                "HTTP/1.1 200 OK",
                r#"{"total":2147483648,"hits":[]}"#.to_owned(),
            );
        }
        if case == "task_ambiguous_title_2xx" {
            return (
                "HTTP/1.1 200 OK",
                task_page_response().replace(
                    r#""title":"contract-base.3mf""#,
                    r#""title":"contract-base.3mf","designTitle":"ambiguous""#,
                ),
            );
        }
        if case == "task_nonempty_cover_2xx" {
            return (
                "HTTP/1.1 200 OK",
                task_page_response().replace(
                    r#""cover":"""#,
                    r#""cover":"https://untrusted.invalid/private.png""#,
                ),
            );
        }
        return ("HTTP/1.1 200 OK", task_page_response().to_owned());
    }
    if first.starts_with("GET /api/v1/plugin/jobs/38191 ") {
        if case == "lifecycle_hub_outage" {
            return (
                "HTTP/1.1 503 Service Unavailable",
                r#"{"error":"hub_unavailable"}"#.to_owned(),
            );
        }
        if case == "lifecycle_sensitive_page_2xx" {
            return (
                "HTTP/1.1 200 OK",
                format!(
                    r#"{{"studio_submission_id":"{DIAGNOSTIC_SECRET}","job_status":"queued","print_status":"pending"}}"#
                ),
            );
        }
        if case == "lifecycle_sensitive_error_4xx" {
            return (
                "HTTP/1.1 400 Bad Request",
                format!(r#"{{"error":{{"detail":"{DIAGNOSTIC_SECRET}"}}}}"#),
            );
        }
        let poll = job_polls.fetch_add(1, Ordering::SeqCst);
        let (state, print_status) = if case == "downstream_failure" {
            ("failed", "pending")
        } else if poll == 0 {
            ("acknowledged", "pending")
        } else if case == "physical_abort_after_publish" {
            ("succeeded", "cancelled")
        } else {
            ("succeeded", "pending")
        };
        return (
            "HTTP/1.1 200 OK",
            format!(
                r#"{{"studio_submission_id":38191,"job_status":"{state}","print_status":"{print_status}"}}"#
            ),
        );
    }
    (
        "HTTP/1.1 404 Not Found",
        r#"{"error":"contract_route_not_found"}"#.to_owned(),
    )
}

fn model_task_response(case: &str) -> (&'static str, String) {
    if case == "model_task_destroy_no_auth_recovery" {
        return (
            "HTTP/1.1 401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#.to_owned(),
        );
    }
    if case == "model_task_metadata_unavailable" {
        return (
            "HTTP/1.1 409 Conflict",
            r#"{"error":"studio_model_task_metadata_unavailable"}"#.to_owned(),
        );
    }
    if case == "model_task_invalid_2xx" {
        return (
            "HTTP/1.1 200 OK",
            r#"{"job_id":38191,"design_id":44,"profile_id":55,"instance_id":38191,"task_id":"38192","model_id":"foreign-model","model_name":"foreign-project","profile_name":"foreign-preset"}"#.to_owned(),
        );
    }
    (
        "HTTP/1.1 200 OK",
        r#"{"job_id":38191,"design_id":0,"profile_id":0,"instance_id":0,"task_id":"38191","model_id":"","model_name":"contract-base-project","profile_name":"contract-base-preset"}"#
            .to_owned(),
    )
}

fn cancel_response(case: &str) -> (&'static str, String) {
    if case == "cancel_too_late" || case == "cancel_after_wait" {
        return (
            "HTTP/1.1 409 Conflict",
            r#"{"error":"cancel_too_late"}"#.to_owned(),
        );
    }
    if case == "cancel_wrong_id" {
        return (
            "HTTP/1.1 200 OK",
            r#"{"studio_submission_id":38192,"job_status":"cancelled","print_status":"cancelled"}"#
                .to_owned(),
        );
    }
    if case == "stale_after_201" || case == "stale_cancel_failed" || case == "cancel_race_stale" {
        return (
            "HTTP/1.1 503 Service Unavailable",
            r#"{"error":"hub_unavailable"}"#.to_owned(),
        );
    }
    (
        "HTTP/1.1 200 OK",
        r#"{"studio_submission_id":38191,"job_status":"cancelled","print_status":"cancelled"}"#
            .to_owned(),
    )
}

fn write_response(stream: &mut impl Write, status: &str, body: &str) -> std::io::Result<()> {
    write!(
        stream,
        "{status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn printer_response() -> &'static str {
    r#"{"message":"success","devices":[{"dev_id":"studio-serial-1","dev_name":"Contract Printer","name":"Contract Printer","dev_ip":null,"dev_access_code":null,"dev_model_name":"N6","model":"N6","dev_online":true,"online":true,"task_status":"IDLE","state":"IDLE","gcode_state":"IDLE","mc_percent":0,"mc_remaining_time":0,"layer_num":0,"total_layer_num":0,"task_id":"","subtask_id":"","gcode_file":"","subtask_name":"","hms":[],"pandar_printer_id":"printer-1","nozzle_temperatures":[],"active_nozzle":null,"bed_temperature_celsius":null,"bed_target_temperature_celsius":null,"chamber_temperature_celsius":null,"chamber_light_on":null,"materials":null}]}"#
}

fn task_page_response() -> &'static str {
    r#"{"total":1,"hits":[{"id":38191,"status":1,"designId":0,"title":"contract-base.3mf","deviceName":"Contract Printer","deviceId":"studio-serial-1","cover":"","startTime":"2026-07-20T12:00:00Z","endTime":"","profileId":38191}]}"#
}

fn subtask_response() -> &'static str {
    r##"{"content":"{\"info\":{\"plate_idx\":7}}","context":{"plates":[{"index":7,"thumbnail":{"url":""},"prediction":3600,"weight":12.5,"filaments":[{"color":"#FFFFFFFF","type":"PLA","used_g":"12.5","used_m":"4.2"}]}]}}"##
}
