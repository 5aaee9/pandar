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

fn read_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).expect("read logout request");
        assert!(read > 0, "logout request ended before headers");
        request.extend_from_slice(&buffer[..read]);
    }
    String::from_utf8(request).expect("logout request is UTF-8")
}

pub(super) fn next_request(listener: &TcpListener, deadline: Instant) -> (TcpStream, String) {
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                let request = read_request(&mut stream);
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
            Ok(_) => panic!("logout sent an unexpected request"),
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
