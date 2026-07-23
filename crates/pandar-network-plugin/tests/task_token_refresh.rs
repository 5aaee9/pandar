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

fn read_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).expect("read mock Hub request");
        assert!(read > 0, "mock Hub request ended before headers");
        request.extend_from_slice(&buffer[..read]);
    }
    String::from_utf8(request).expect("mock Hub request is UTF-8")
}

fn next_request(listener: &TcpListener, deadline: Instant) -> (TcpStream, String) {
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                let request = read_request(&mut stream);
                return (stream, request);
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
            Ok(_) => panic!("task read retried token rotation more than once"),
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

#[test]
fn task_list_rotates_a_rejected_no_auth_token_once_and_retries_with_a_fresh_snapshot() {
    let output = run_probe("tasks", |listener, deadline| {
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(
            &request,
            "GET",
            "/api/v1/plugin/jobs?dev_id=&status=0&offset=0&limit=20",
            Some("stale-token"),
        );
        write_response(
            &mut stream,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        rotate_session(&listener, deadline, "fresh-token");
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(
            &request,
            "GET",
            "/api/v1/plugin/jobs?dev_id=&status=0&offset=0&limit=20",
            Some("fresh-token"),
        );
        write_response(&mut stream, "200 OK", r#"{"total":0,"hits":[]}"#);
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"tasks"}"#);
}

#[test]
fn task_plate_rotates_a_gone_no_auth_token_once_and_retries_with_a_fresh_snapshot() {
    let output = run_probe("plate", |listener, deadline| {
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(
            &request,
            "GET",
            "/api/v1/plugin/jobs/42/plate",
            Some("stale-token"),
        );
        write_response(&mut stream, "410 Gone", r#"{"error":"expired_auth_token"}"#);
        rotate_session(&listener, deadline, "plate-token");
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(
            &request,
            "GET",
            "/api/v1/plugin/jobs/42/plate",
            Some("plate-token"),
        );
        write_response(
            &mut stream,
            "200 OK",
            r#"{"studio_submission_id":42,"plate_index":3}"#,
        );
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"plate"}"#);
}

#[test]
fn subtask_rotates_a_rejected_no_auth_token_once_and_retries_with_a_fresh_snapshot() {
    let output = run_probe("subtask", |listener, deadline| {
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(
            &request,
            "GET",
            "/api/v1/plugin/jobs/42/subtask",
            Some("stale-token"),
        );
        write_response(
            &mut stream,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        rotate_session(&listener, deadline, "subtask-token");
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(
            &request,
            "GET",
            "/api/v1/plugin/jobs/42/subtask",
            Some("subtask-token"),
        );
        write_response(
            &mut stream,
            "200 OK",
            r##"{"content":"{\"info\":{\"plate_idx\":3}}","context":{"plates":[{"index":3,"thumbnail":{"url":""},"prediction":120,"weight":12.5,"filaments":[{"color":"#FFFFFFFF","type":"PLA","used_g":"12.5","used_m":"4.2"}]}]}}"##,
        );
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"subtask"}"#);
}

#[test]
fn task_read_does_not_rotate_or_retry_again_after_the_single_retry_is_rejected() {
    let output = run_probe("retry-rejected", |listener, deadline| {
        let path = "/api/v1/plugin/jobs?dev_id=&status=0&offset=0&limit=20";
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(&request, "GET", path, Some("stale-token"));
        write_response(
            &mut stream,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        rotate_session(&listener, deadline, "single-retry-token");
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(&request, "GET", path, Some("single-retry-token"));
        write_response(&mut stream, "410 Gone", r#"{"error":"expired_auth_token"}"#);
        assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"retry-rejected"}"#);
}

#[test]
fn task_read_reports_the_no_auth_rotation_failure_instead_of_the_stale_401() {
    let output = run_probe("rotation-failure", |listener, deadline| {
        let path = "/api/v1/plugin/jobs?dev_id=&status=0&offset=0&limit=20";
        let (mut stream, request) = next_request(&listener, deadline);
        assert_request(&request, "GET", path, Some("stale-token"));
        write_response(
            &mut stream,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );

        let (mut rotation, request) = next_request(&listener, deadline);
        assert_request(&request, "POST", "/api/v1/plugin/no-auth-session", None);
        write_response(
            &mut rotation,
            "409 Conflict",
            r#"{"error":"ambiguous_no_auth_tenant"}"#,
        );
        assert_no_request(&listener, Instant::now() + Duration::from_millis(300));
    });
    assert_eq!(output.trim(), r#"{"ok":true,"mode":"rotation-failure"}"#);
}
