use std::{
    env,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

struct CompiledFixture {
    executable: PathBuf,
    _directory: tempfile::TempDir,
}

fn target_dir() -> PathBuf {
    env::var_os("CARGO_TARGET_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                env::current_dir().unwrap().join(path)
            }
        })
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(Path::parent)
                .unwrap()
                .join("target")
        })
}

fn build_plugin() -> PathBuf {
    let output = Command::new("cargo")
        .args(["build", "-p", "pandar-network-plugin"])
        .output()
        .expect("build network plugin for logout revoke probe");
    assert!(
        output.status.success(),
        "plugin build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_owned());
    let filename = if cfg!(windows) {
        "pandar_network_plugin.dll"
    } else if cfg!(target_os = "macos") {
        "libpandar_network_plugin.dylib"
    } else {
        "libpandar_network_plugin.so"
    };
    target_dir().join(profile).join(filename)
}

fn compile_fixture(source: &str) -> CompiledFixture {
    let directory = tempfile::tempdir().expect("create logout revoke compiler directory");
    let executable_name = Path::new(source).file_stem().unwrap();
    let executable = directory
        .path()
        .join(executable_name)
        .with_extension(if cfg!(windows) { "exe" } else { "" });
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(source);

    #[cfg(all(windows, target_env = "msvc"))]
    let (mut command, compiler) = {
        let tool = cc::windows_registry::find_tool(env::consts::ARCH, "cl.exe")
            .expect("MSVC cl.exe is required for logout revoke probe");
        let compiler = tool.path().display().to_string();
        (tool.to_command(), compiler)
    };
    #[cfg(not(all(windows, target_env = "msvc")))]
    let (mut command, compiler) = (Command::new("c++"), "c++".to_owned());

    if cfg!(target_env = "msvc") {
        command
            .arg("/nologo")
            .arg("/std:c++17")
            .arg("/EHsc")
            .arg("/MD")
            .arg("/D_ITERATOR_DEBUG_LEVEL=0")
            .arg(&fixture)
            .arg(format!("/Fe{}", executable.display()))
            .arg(format!("/Fo{}", executable.with_extension("obj").display()));
    } else {
        command
            .arg("-std=c++17")
            .arg(&fixture)
            .arg("-o")
            .arg(&executable);
        if cfg!(target_os = "linux") {
            command.arg("-ldl");
        }
    }
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("launch {compiler}: {error}"));
    assert!(
        output.status.success(),
        "fixture compile failed with {compiler}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    CompiledFixture {
        executable,
        _directory: directory,
    }
}

/// Reads one HTTP request. Returns `None` when the peer closed the
/// connection without sending any bytes; the plugin's stream worker can
/// abandon a dial when its account episode changes mid-connect.
fn read_request(stream: &mut TcpStream) -> Option<String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).expect("read logout request");
        if read == 0 {
            if request.is_empty() {
                return None;
            }
            panic!("logout request ended before headers");
        }
        request.extend_from_slice(&buffer[..read]);
    }
    Some(String::from_utf8(request).expect("logout request is UTF-8"))
}

fn is_printer_events_upgrade(request: &str) -> bool {
    let request_line = request.lines().next().unwrap_or_default();
    let upgrade_header = request.lines().any(|line| {
        line.split_once(':')
            .is_some_and(|(name, _)| name.eq_ignore_ascii_case("upgrade"))
    });
    request_line.starts_with("GET /api/v1/tenants/")
        && request_line.contains("/printer-events?")
        && upgrade_header
}

/// Answers a printer-events upgrade with a 101 handshake, seeds the cache
/// with the scripted snapshot, and keeps the socket alive in the background.
fn serve_stream_upgrade(mut stream: TcpStream, request: &str) {
    use std::sync::atomic::AtomicBool;

    let key = request.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("sec-websocket-key")
            .then(|| value.trim().to_owned())
    });
    let Some(key) = key else { return };
    let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
    let handshake = format!(
        "HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    if stream.write_all(handshake.as_bytes()).is_err() {
        return;
    }
    let mut ws =
        tungstenite::WebSocket::from_raw_socket(stream, tungstenite::protocol::Role::Server, None);
    let _ = ws.get_ref().set_nonblocking(true);
    let response: serde_json::Value = serde_json::from_str(PRINTERS_RESPONSE).unwrap();
    let mut frames = vec![r#"{"type":"snapshot_begin","version":1}"#.to_owned()];
    for device in response["devices"].as_array().unwrap() {
        frames.push(format!(r#"{{"type":"printer_upsert","printer":{device}}}"#));
    }
    frames.push(r#"{"type":"snapshot_end","version":1}"#.to_owned());
    for frame in frames {
        if ws.write(tungstenite::Message::text(frame)).is_err() {
            return;
        }
        let _ = ws.flush();
    }
    loop {
        if ws.read().is_ok() {
            let _ = ws.flush();
        }
        std::thread::sleep(Duration::from_millis(50));
        let _ = &AtomicBool::new(false);
    }
}

pub(super) fn next_request(listener: &TcpListener, deadline: Instant) -> (TcpStream, String) {
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                let Some(request) = read_request(&mut stream) else {
                    continue;
                };
                if is_printer_events_upgrade(&request) {
                    thread::spawn(move || serve_stream_upgrade(stream, &request));
                    continue;
                }
                return (stream, request);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "timed out waiting for DELETE");
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("accept logout request: {error}"),
        }
    }
}
pub(super) fn assert_no_request(listener: &TcpListener, deadline: Instant) {
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                let Some(request) = read_request(&mut stream) else {
                    continue;
                };
                if is_printer_events_upgrade(&request) {
                    thread::spawn(move || serve_stream_upgrade(stream, &request));
                    continue;
                }
                panic!("logout sent an unexpected request: {request}");
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("check for unexpected logout request: {error}"),
        }
    }
}

pub(super) fn write_response(stream: &mut TcpStream, status: &str, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

pub(super) fn wait_for_client_close(mut stream: TcpStream, deadline: Instant) {
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let mut byte = [0_u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => return,
            Ok(_) => panic!("logout sent an unexpected request body"),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionAborted | std::io::ErrorKind::ConnectionReset
                ) =>
            {
                return;
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                assert!(
                    Instant::now() < deadline,
                    "logout client did not close the unanswered DELETE within its bound"
                );
            }
            Err(error) => panic!("wait for logout client close: {error}"),
        }
    }
}

pub(super) fn run_probe(
    mode: &str,
    serve: impl FnOnce(TcpListener, Instant, PathBuf) + Send + 'static,
) -> String {
    run_fixture_probe("logout_revoke_probe.cpp", mode, serve)
}

pub(super) fn run_fixture_probe(
    source: &str,
    mode: &str,
    serve: impl FnOnce(TcpListener, Instant, PathBuf) + Send + 'static,
) -> String {
    let fixture = compile_fixture(source);
    let library = build_plugin();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = tempfile::tempdir().unwrap();
    let server_config = config.path().to_owned();
    let server = thread::spawn(move || {
        serve(
            listener,
            Instant::now() + Duration::from_secs(5),
            server_config,
        );
    });
    let output = Command::new(&fixture.executable)
        .arg(library)
        .arg(mode)
        .arg(config.path())
        .env("PANDAR_PLUGIN_HUB_URL", &url)
        .env("PANDAR_PLUGIN_FRONTEND_URL", "http://127.0.0.1:3000")
        .output()
        .expect("run logout revoke probe");
    let server_result = server.join();
    assert!(server_result.is_ok(), "mock Hub failed: {server_result:?}");
    assert!(
        output.status.success(),
        "probe failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let output_text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output_text.contains("logout-secret-token"));
    assert!(!output_text.contains("reentrant-upgrade-token"));
    assert!(!output_text.contains("late-no-auth-secret-token"));
    assert!(!output_text.contains("late-ticket-after-passive-token"));
    assert!(!output_text.contains("pending-bootstrap-token"));
    assert!(!output_text.contains("requested-race-ticket"));
    assert!(!output_text.contains("passive-ticket"));
    assert!(!output_text.contains("late-ticket-token"));
    assert!(!output_text.contains("passive-ticket-token"));
    assert!(!output_text.contains("late-passive-ticket"));
    assert!(!output_text.contains("replacement-token"));
    assert!(!output_text.contains(&url));
    assert!(!output_text.contains("raw-logout-failure"));
    assert!(!output_text.contains("raw-upgrade-delete-failure"));
    assert!(!output_text.contains("raw-stage-delete-failure"));
    if matches!(
        mode,
        "disconnect" | "timeout" | "timeout-relogin" | "reentrant-retained-disconnect"
    ) {
        assert!(output_text.contains("DELETE plugin session request"));
        let lower = output_text.to_ascii_lowercase();
        if mode == "disconnect" || mode == "reentrant-retained-disconnect" {
            assert!(
                lower.contains("connection closed")
                    || lower.contains("unexpected eof")
                    || lower.contains("connection reset"),
                "disconnect diagnostic lacked its lower-level cause: {output_text}"
            );
        } else {
            assert!(
                lower.contains("timed out") || lower.contains("timeout"),
                "timeout diagnostic lacked its lower-level cause: {output_text}"
            );
        }
    }
    if matches!(
        mode,
        "stage-failure-delete-success"
            | "stage-failure-delete-delayed-success"
            | "stage-failure-delete-failure"
            | "stage-failure-delete-relogin-success"
            | "stage-failure-delete-relogin-failure"
            | "stage-failure-delete-unauthorized"
            | "stage-failure-delete-gone"
            | "reentrant-retained-failure"
            | "reentrant-retained-disconnect"
    ) {
        assert!(output_text.contains("stage pending plugin session revocation"));
        assert!(output_text.contains("read pending plugin revocations"));
    }
    String::from_utf8(output.stdout).unwrap()
}

pub(super) const PRINTERS_RESPONSE: &str = r#"{"message":"success","devices":[{"dev_id":"logout-printer","dev_name":"Logout Printer","name":"Logout Printer","dev_model_name":"N6","model":"N6","dev_online":true,"online":true,"task_status":"IDLE","state":"IDLE","gcode_state":"IDLE","hms":[],"pandar_printer_id":"printer-1","nozzle_temperatures":[],"active_nozzle":null,"bed_temperature_celsius":null,"bed_target_temperature_celsius":null,"chamber_temperature_celsius":null,"chamber_light_on":null,"materials":null}]}"#;
