use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::{Duration, Instant},
};

pub(super) const PREPARED: &str =
    r#"{"command_id":"00000000-0000-0000-0000-000000000001","prepared_token":"prepared"}"#;

pub(super) struct Response {
    status: &'static str,
    body: String,
}

impl Response {
    pub(super) fn json(status: &'static str, body: impl Into<String>) -> Self {
        Self {
            status,
            body: body.into(),
        }
    }
}

pub(super) fn mock_hub(responses: Vec<Response>) -> (String, thread::JoinHandle<Vec<String>>) {
    spawn_hub(responses, Duration::from_secs(2))
}

pub(super) fn probe_hub(responses: Vec<Response>) -> (String, thread::JoinHandle<Vec<String>>) {
    spawn_hub(responses, Duration::from_millis(300))
}

fn spawn_hub(
    responses: Vec<Response>,
    lifetime: Duration,
) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let deadline = Instant::now() + lifetime;
        let mut responses = VecDeque::from(responses);
        let mut requests = Vec::new();
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    requests.push(read_request(&mut stream));
                    if let Some(response) = responses.pop_front() {
                        respond(&mut stream, response);
                    }
                    if responses.is_empty() {
                        break;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("mock Hub accept failed: {error}"),
            }
        }
        requests
    });
    (url, handle)
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    let headers_end = loop {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0);
        request.extend_from_slice(&buffer[..read]);
        if let Some(position) = request.windows(4).position(|value| value == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let headers = String::from_utf8_lossy(&request[..headers_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .and_then(|value| value.parse::<usize>().ok())
        })
        .unwrap_or(0);
    while request.len() - headers_end < content_length {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0);
        request.extend_from_slice(&buffer[..read]);
    }
    String::from_utf8(request).unwrap()
}

fn respond(stream: &mut TcpStream, response: Response) {
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.status,
        response.body.len(),
        response.body
    );
    stream.write_all(response.as_bytes()).unwrap();
}

pub(super) fn acknowledged(sequence_id: &str) -> String {
    format!(
        r#"{{"command_id":"00000000-0000-0000-0000-000000000001","phase":"acknowledged","outcome":{{"outcome":"acknowledged","acknowledgement":{{"command":"upgrade_confirm","sequence_id":"{sequence_id}","result":"success"}}}}}}"#
    )
}

pub(super) fn command(sequence_id: &str) -> String {
    format!(
        r#"{{"upgrade":{{"command":"upgrade_confirm","sequence_id":"{sequence_id}","src_id":1}}}}"#
    )
}

pub(super) fn printer_batch() -> &'static str {
    r#"{"message":"success","devices":[{
        "dev_id":"SERIAL","dev_name":"Printer","name":"Printer",
        "dev_model_name":"N6","model":"N6","dev_online":true,
        "online":true,"task_status":"IDLE","state":"IDLE","gcode_state":"IDLE",
        "mc_percent":0,"mc_remaining_time":0,"layer_num":0,"total_layer_num":0,
        "task_id":null,"print_error":null,"job_id":null,"subtask_id":null,
        "gcode_file":null,"subtask_name":null,"hms":[],"pandar_printer_id":"printer-1",
        "nozzle_temperatures":[],"active_nozzle":null,"bed_temperature_celsius":null,
        "bed_target_temperature_celsius":null,"chamber_temperature_celsius":null,
        "chamber_light_on":null,"materials":null,
        "firmware":{"session_id":"session-1","generation":2,"module_revision":8,
            "status_revision":9,"modules":[{"name":"ota","sw_ver":"01.02.03.04"}],
            "upgrade_state":{"status":"IDLE","progress":"0"},"cfg":"101"}
    }]}"#
}
