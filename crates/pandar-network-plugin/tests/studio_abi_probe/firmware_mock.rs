#[path = "firmware_mock/responses.rs"]
mod responses;

use std::{
    collections::{BTreeSet, HashMap},
    fs,
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use crate::support::read_http_request_with_timeout;

use self::responses::{
    catalog_response, is_delayed_refresh, json_body, login_response, printer_list,
    printer_list_with_version, refresh_response, respond, string_field,
};

const START_URL: &str = "https://user:secret@example.invalid/fw.bin?sig=ABI_SENTINEL";

pub(super) struct FirmwareMockHub {
    pub(super) url: String,
    stop: Arc<AtomicBool>,
    handle: thread::JoinHandle<State>,
}

#[derive(Default)]
struct State {
    catalog_reads: usize,
    printer_reads: usize,
    delayed_refresh_stream: Option<(TcpStream, Instant)>,
    refresh_sequences: BTreeSet<String>,
    prepared: HashMap<String, Value>,
    prepare_keys: BTreeSet<(String, String)>,
    execute_keys: BTreeSet<(String, String)>,
}

impl FirmwareMockHub {
    pub(super) fn finish(self, require_complete: bool) -> thread::Result<()> {
        self.stop.store(true, Ordering::Release);
        let state = self.handle.join()?;
        if require_complete {
            state.assert_complete();
        }
        Ok(())
    }
}

pub(super) fn spawn_firmware_mock_hub(race_directory: &Path) -> FirmwareMockHub {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let race_directory = race_directory.to_owned();
    let handle = thread::spawn(move || {
        let mut state = State::default();
        while !thread_stop.load(Ordering::Acquire) {
            state.complete_delayed_refresh();
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let mut first_byte = [0_u8; 1];
                    match stream.peek(&mut first_byte) {
                        Ok(0) => continue,
                        Ok(_) => {}
                        Err(error) => {
                            panic!("firmware mock failed waiting for request bytes: {error}")
                        }
                    }
                    let request =
                        read_http_request_with_timeout(&mut stream, Some(Duration::from_secs(5)));
                    if request.lines().next().unwrap_or_default()
                        == "GET /api/v1/plugin/printers HTTP/1.1"
                    {
                        state.handle_printers(stream, &request, &race_directory);
                    } else if is_delayed_refresh(&request) {
                        state.handle_delayed_refresh(stream, &request, &race_directory);
                    } else {
                        state.handle(&mut stream, &request);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("firmware mock accept failed: {error}"),
            }
        }
        state
    });
    FirmwareMockHub { url, stop, handle }
}

impl State {
    fn complete_delayed_refresh(&mut self) {
        let Some((_, deadline)) = self.delayed_refresh_stream.as_ref() else {
            return;
        };
        if Instant::now() < *deadline {
            return;
        }
        let (mut stream, _) = self.delayed_refresh_stream.take().unwrap();
        respond(&mut stream, "HTTP/1.1 200 OK", refresh_response());
    }

    fn handle_delayed_refresh(&mut self, stream: TcpStream, request: &str, race_directory: &Path) {
        assert!(
            request.contains("authorization: Bearer probe-token"),
            "firmware request lacked plugin auth: {request}"
        );
        let body = json_body(request);
        let sequence = string_field(&body, "sequence_id");
        assert_eq!(sequence, "c-lock-overlap-version");
        self.refresh_sequences.insert(sequence);
        fs::create_dir(race_directory.join("slow-version-refresh-entered")).unwrap();
        assert!(
            self.delayed_refresh_stream
                .replace((stream, Instant::now() + Duration::from_millis(2_300)))
                .is_none()
        );
    }

    fn handle_printers(&mut self, mut stream: TcpStream, request: &str, race_directory: &Path) {
        assert!(
            request.contains("authorization: Bearer probe-token"),
            "firmware request lacked plugin auth: {request}"
        );
        self.printer_reads += 1;
        let heartbeat_arm = race_directory.join("version-heartbeat-arm");
        let version_heartbeat_armed = heartbeat_arm.exists();
        if version_heartbeat_armed {
            respond(
                &mut stream,
                "HTTP/1.1 200 OK",
                printer_list_with_version("version-heartbeat", "09.87.65.43"),
            );
        } else {
            match self.printer_reads {
                1 | 3 => respond(
                    &mut stream,
                    "HTTP/1.1 200 OK",
                    printer_list_with_version("session-auxiliary", "08.08.08.08"),
                ),
                2 => {
                    fs::create_dir(race_directory.join("background-printer-failure-served"))
                        .unwrap();
                    respond(
                        &mut stream,
                        "HTTP/1.1 503 Service Unavailable",
                        r#"{"error":"observe cached firmware"}"#.to_owned(),
                    );
                }
                _ => respond(&mut stream, "HTTP/1.1 200 OK", printer_list()),
            }
        }
        if version_heartbeat_armed {
            fs::remove_dir(heartbeat_arm).unwrap();
        }
    }

    fn handle(&mut self, stream: &mut TcpStream, request: &str) {
        let line = request.lines().next().unwrap_or_default();
        if line == "POST /api/v1/plugin/no-auth-session HTTP/1.1" {
            return respond(stream, "HTTP/1.1 200 OK", login_response());
        }
        assert!(
            request.contains("authorization: Bearer probe-token"),
            "firmware request lacked plugin auth: {request}"
        );
        match line {
            "GET /api/v1/plugin/printers HTTP/1.1" => {
                respond(stream, "HTTP/1.1 200 OK", printer_list())
            }
            "GET /api/v1/plugin/printers/printer-1/firmware HTTP/1.1" => {
                self.catalog_reads += 1;
                let body = catalog_response(self.catalog_reads > 1);
                respond(stream, "HTTP/1.1 200 OK", body)
            }
            "POST /api/v1/plugin/printers/printer-1/firmware/refresh HTTP/1.1" => {
                let body = json_body(request);
                let sequence = string_field(&body, "sequence_id");
                self.refresh_sequences.insert(sequence.clone());
                respond(stream, "HTTP/1.1 200 OK", refresh_response())
            }
            "POST /api/v1/plugin/printers/printer-1/firmware/prepare HTTP/1.1" => {
                self.prepare(stream, request)
            }
            "POST /api/v1/plugin/printers/printer-1/firmware/execute HTTP/1.1" => {
                self.execute(stream, request)
            }
            _ => panic!("unexpected firmware ABI request: {request}"),
        }
    }

    fn prepare(&mut self, stream: &mut TcpStream, request: &str) {
        let command = json_body(request);
        assert!(
            command.get("url").is_none(),
            "prepare leaked URL: {command}"
        );
        let name = string_field(&command, "command");
        let sequence = string_field(&command, "sequence_id");
        assert_command(&command, &name, &sequence, false);
        let token = format!("prepared-{sequence}");
        assert!(self.prepared.insert(token.clone(), command).is_none());
        self.prepare_keys.insert((name, sequence));
        respond(
            stream,
            "HTTP/1.1 200 OK",
            json!({
                "command_id":"00000000-0000-0000-0000-000000000011",
                "prepared_token":token
            })
            .to_string(),
        );
    }

    fn execute(&mut self, stream: &mut TcpStream, request: &str) {
        let body = json_body(request);
        let token = string_field(&body, "prepared_token");
        let command = body.get("command").cloned().expect("execute command");
        let name = string_field(&command, "command");
        let sequence = string_field(&command, "sequence_id");
        assert_command(&command, &name, &sequence, true);
        let prepared = self.prepared.remove(&token).expect("known prepared token");
        let mut expected = command.clone();
        expected.as_object_mut().unwrap().remove("url");
        assert_eq!(prepared, expected, "prepare/execute metadata changed");
        self.execute_keys.insert((name.clone(), sequence.clone()));
        if sequence == "c-delay-reject" {
            thread::sleep(Duration::from_millis(350));
        }
        let response = if matches!(
            sequence.as_str(),
            "c-delay-reject"
                | "c-deadline"
                | "c-reentrant"
                | "c-lock-order"
                | "c-lock-overlap-ack"
                | "c-logout"
                | "c-destroy"
                | "l-generation-fence"
        ) {
            json!({
                "command_id":"00000000-0000-0000-0000-000000000011",
                "phase":"rejected",
                "outcome":{"outcome":"acknowledged","acknowledgement":{
                    "command":name,"sequence_id":sequence,"result":"fail","err_code":765,
                    "reason":"printer_busy","message":"selector rejected"
                }},
                "transient_status":{"upgrade_state":{"status":"FAIL","progress":"42"},"cfg":"101"}
            })
        } else {
            json!({
                "command_id":"00000000-0000-0000-0000-000000000011",
                "phase":"outcome_unknown",
                "outcome":{"outcome":"published_without_acknowledgement"}
            })
        };
        respond(stream, "HTTP/1.1 200 OK", response.to_string());
    }

    fn assert_complete(&self) {
        assert!(
            self.printer_reads >= 4,
            "printer failure and recovery were not exercised"
        );
        assert_eq!(
            self.catalog_reads, 2,
            "catalog was not read empty then populated"
        );
        assert_eq!(
            self.refresh_sequences,
            BTreeSet::from([
                "c-generation-fence".into(),
                "c-synchronous-reentrant".into(),
                "c-version".into(),
                "c-lock-overlap-version".into(),
                "l-version".into(),
            ])
        );
        let expected = expected_commands();
        assert_eq!(self.prepare_keys, expected, "prepare command coverage");
        assert_eq!(self.execute_keys, expected, "execute command coverage");
        assert!(
            self.prepared.is_empty(),
            "prepared command was not executed"
        );
        assert!(self.delayed_refresh_stream.is_none());
    }
}

fn assert_command(command: &Value, name: &str, sequence: &str, execute: bool) {
    let prefix = sequence.split('-').next().unwrap_or_default();
    let expected = match name {
        "upgrade_confirm" => json!({"command":name,"sequence_id":sequence,"src_id":1}),
        "consistency_confirm" => json!({"command":name,"sequence_id":sequence,"src_id":2}),
        "start" => json!({
            "command":name,"sequence_id":sequence,"src_id":3,"module":"n3s/0",
            "version":"03.04.05.06","url":START_URL
        }),
        "mc_for_ams_firmware_upgrade" => {
            json!({"command":name,"sequence_id":sequence,"src_id":4,"id":-7})
        }
        _ => panic!("unexpected firmware command {name}"),
    };
    let mut expected = expected;
    if !execute {
        expected.as_object_mut().unwrap().remove("url");
    }
    assert_eq!(command, &expected, "{prefix} {name} body changed");
}

fn expected_commands() -> BTreeSet<(String, String)> {
    [
        ("upgrade_confirm", "c-upgrade"),
        ("consistency_confirm", "c-consistency"),
        ("start", "c-start"),
        ("mc_for_ams_firmware_upgrade", "c-delay-reject"),
        ("upgrade_confirm", "c-deadline"),
        ("upgrade_confirm", "l-upgrade"),
        ("consistency_confirm", "l-consistency"),
        ("start", "l-start"),
        ("mc_for_ams_firmware_upgrade", "l-switch"),
        ("upgrade_confirm", "c-reentrant"),
        ("upgrade_confirm", "c-lock-order"),
        ("upgrade_confirm", "c-lock-overlap-ack"),
        ("upgrade_confirm", "c-logout"),
        ("upgrade_confirm", "c-destroy"),
        ("upgrade_confirm", "l-generation-fence"),
    ]
    .into_iter()
    .map(|(command, sequence)| (command.into(), sequence.into()))
    .collect()
}
