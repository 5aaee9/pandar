#![cfg(any(unix, windows))]

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
        .expect("build network plugin for task token refresh probe");
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

fn compile_fixture() -> CompiledFixture {
    let directory = tempfile::tempdir().expect("create task token refresh compiler directory");
    let executable = directory.path().join(if cfg!(windows) {
        "task_token_refresh_probe.exe"
    } else {
        "task_token_refresh_probe"
    });
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/task_token_refresh_probe.cpp");

    #[cfg(all(windows, target_env = "msvc"))]
    let (mut command, compiler) = {
        let tool = cc::windows_registry::find_tool(env::consts::ARCH, "cl.exe")
            .expect("MSVC cl.exe is required for task token refresh probe");
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

/// Reads one HTTP request head. Returns `None` when the peer closed the
/// connection without sending any bytes; the plugin's stream worker can
/// abandon a dial when its account episode changes mid-connect.
fn read_request(stream: &mut TcpStream) -> Option<String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).expect("read mock Hub request");
        if read == 0 {
            if request.is_empty() {
                return None;
            }
            panic!("mock Hub request ended before headers");
        }
        request.extend_from_slice(&buffer[..read]);
    }
    Some(String::from_utf8(request).expect("mock Hub request is UTF-8"))
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

/// Answers a printer-events upgrade with a 101 handshake and an empty
/// snapshot, then keeps the socket alive until the peer goes away.
fn serve_stream_upgrade(mut stream: TcpStream, request: &str) {
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
    for frame in [
        r#"{"type":"snapshot_begin","version":1}"#,
        r#"{"type":"snapshot_end","version":1}"#,
    ] {
        if ws.write(tungstenite::Message::text(frame)).is_err() {
            return;
        }
        let _ = ws.flush();
    }
    loop {
        match ws.read() {
            Ok(_) => {
                let _ = ws.flush();
            }
            Err(tungstenite::Error::Io(error))
                if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return,
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn next_request(listener: &TcpListener, deadline: Instant) -> (TcpStream, String) {
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                let Some(text) = read_request(&mut stream) else {
                    continue;
                };
                if is_printer_events_upgrade(&text) {
                    thread::spawn(move || serve_stream_upgrade(stream, &text));
                    continue;
                }
                return (stream, text);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "timed out waiting for mock Hub request"
                );
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("accept mock Hub request: {error}"),
        }
    }
}

fn assert_no_request(listener: &TcpListener, deadline: Instant) {
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                let Some(text) = read_request(&mut stream) else {
                    continue;
                };
                if is_printer_events_upgrade(&text) {
                    thread::spawn(move || serve_stream_upgrade(stream, &text));
                    continue;
                }
                panic!("task read retried token rotation more than once: {text}");
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("check for unexpected mock Hub request: {error}"),
        }
    }
}

fn write_response(stream: &mut TcpStream, status: &str, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

fn assert_request(request: &str, method: &str, path: &str, token: Option<&str>) {
    assert!(
        request.starts_with(&format!("{method} {path}")),
        "unexpected request line: {request}"
    );
    if let Some(token) = token {
        assert!(
            request
                .to_ascii_lowercase()
                .contains(&format!("authorization: bearer {token}")),
            "request omitted expected bearer token: {request}"
        );
    }
}

fn run_probe(mode: &str, serve: impl FnOnce(TcpListener, Instant) + Send + 'static) -> String {
    let fixture = compile_fixture();
    let library = build_plugin();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        serve(listener, deadline);
    });

    let config = tempfile::tempdir().unwrap();
    let output = Command::new(&fixture.executable)
        .arg(library)
        .arg(mode)
        .arg(config.path())
        .env("PANDAR_PLUGIN_HUB_URL", url)
        .env("PANDAR_PLUGIN_FRONTEND_URL", "http://127.0.0.1:3000")
        .output()
        .expect("run task token refresh probe");
    let server_result = server.join();
    assert!(server_result.is_ok(), "mock Hub failed: {server_result:?}");
    assert!(
        output.status.success(),
        "probe failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn rotate_session(listener: &TcpListener, deadline: Instant, fresh_token: &str) {
    let (mut stream, request) = next_request(listener, deadline);
    assert_request(&request, "POST", "/api/v1/plugin/no-auth-session", None);
    write_response(
        &mut stream,
        "200 OK",
        &format!(
            r#"{{"token":"{fresh_token}","profile":{{"token":"{fresh_token}","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"}}}}"#
        ),
    );
}

#[path = "task_token_refresh/cases.rs"]
mod cases;
