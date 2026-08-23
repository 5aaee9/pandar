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

struct Fixture {
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
        .expect("build plugin for credential hygiene probe");
    assert!(
        output.status.success(),
        "plugin build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let filename = if cfg!(windows) {
        "pandar_network_plugin.dll"
    } else if cfg!(target_os = "macos") {
        "libpandar_network_plugin.dylib"
    } else {
        "libpandar_network_plugin.so"
    };
    target_dir()
        .join(env::var("PROFILE").unwrap_or_else(|_| "debug".to_owned()))
        .join(filename)
}

fn compile_fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join(if cfg!(windows) {
        "no_auth_credential_hygiene_probe.exe"
    } else {
        "no_auth_credential_hygiene_probe"
    });
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/no_auth_credential_hygiene_probe.cpp");
    #[cfg(all(windows, target_env = "msvc"))]
    let (mut command, compiler) = {
        let tool = cc::windows_registry::find_tool(env::consts::ARCH, "cl.exe")
            .expect("MSVC cl.exe is required for credential hygiene probe");
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
            .arg(&source)
            .arg(format!("/Fe{}", executable.display()))
            .arg(format!("/Fo{}", executable.with_extension("obj").display()));
    } else {
        command
            .arg("-std=c++17")
            .arg(&source)
            .arg("-o")
            .arg(&executable);
        if cfg!(target_os = "linux") {
            command.arg("-ldl");
        }
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "fixture compile failed with {compiler}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Fixture {
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
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !bytes.windows(4).any(|value| value == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).unwrap();
        if read == 0 {
            if bytes.is_empty() {
                return None;
            }
            panic!("request ended before headers");
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    Some(String::from_utf8(bytes).unwrap())
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

/// Accepts the next plain HTTP request, servicing stream upgrades and
/// abandoned dials along the way.
fn request(listener: &TcpListener, deadline: Instant) -> (TcpStream, String) {
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
                assert!(Instant::now() < deadline, "timed out waiting for mock Hub");
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("accept mock Hub request: {error}"),
        }
    }
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
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
        "{request}"
    );
    if let Some(token) = token {
        assert!(
            request
                .to_ascii_lowercase()
                .contains(&format!("authorization: bearer {token}")),
            "request omitted expected token: {request}"
        );
    }
}

fn no_more_requests(listener: &TcpListener, duration: Duration) {
    let deadline = Instant::now() + duration;
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
                panic!("unexpected request: {text}");
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return;
                }
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("check unexpected request: {error}"),
        }
    }
}
fn candidate(token: &str) -> String {
    format!(
        r#"{{"token":"{token}","profile":{{"token":"{token}","user_id":"candidate","user_name":"Candidate","tenant_id":"tenant-1","tenant_name":"Tenant"}}}}"#
    )
}

fn run(mode: &str, serve: impl FnOnce(TcpListener, Instant, PathBuf) + Send + 'static) {
    let fixture = compile_fixture();
    let library = build_plugin();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = tempfile::tempdir().unwrap();
    let server_config = config.path().to_owned();
    let server = thread::spawn(move || {
        serve(
            listener,
            Instant::now() + Duration::from_secs(8),
            server_config,
        );
    });
    let output = Command::new(&fixture.executable)
        .arg(library)
        .arg(mode)
        .arg(config.path())
        .env("PANDAR_PLUGIN_HUB_URL", url)
        .env("PANDAR_PLUGIN_FRONTEND_URL", "http://127.0.0.1:3000")
        .output()
        .unwrap();
    let server_result = server.join();
    assert!(
        server_result.is_ok(),
        "mock Hub failed: {server_result:?}\nprobe stdout:\n{}\nprobe stderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "probe failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        format!(r#"{{"ok":true,"mode":"{mode}"}}"#)
    );
}

#[path = "no_auth_credential_hygiene/cases.rs"]
mod cases;
