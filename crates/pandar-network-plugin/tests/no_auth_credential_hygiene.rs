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

fn request(listener: &TcpListener, deadline: Instant) -> (TcpStream, String) {
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0_u8; 1024];
                while !bytes.windows(4).any(|value| value == b"\r\n\r\n") {
                    let read = stream.read(&mut buffer).unwrap();
                    assert!(read > 0, "request ended before headers");
                    bytes.extend_from_slice(&buffer[..read]);
                }
                return (stream, String::from_utf8(bytes).unwrap());
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
                let mut bytes = [0_u8; 256];
                let read = stream.read(&mut bytes).unwrap_or_default();
                panic!(
                    "unexpected request: {}",
                    String::from_utf8_lossy(&bytes[..read])
                );
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

#[test]
fn concurrent_task_401_responses_share_one_no_auth_rotation() {
    run("concurrent", |listener, deadline, _| {
        let path = "/api/v1/plugin/jobs?dev_id=&status=0&offset=0&limit=20";
        let (mut first, first_request) = request(&listener, deadline);
        let (mut second, second_request) = request(&listener, deadline);
        assert_request(&first_request, "GET", path, Some("stale-token"));
        assert_request(&second_request, "GET", path, Some("stale-token"));
        respond(
            &mut first,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        respond(
            &mut second,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        let (mut rotate, rotate_request) = request(&listener, deadline);
        assert_request(
            &rotate_request,
            "POST",
            "/api/v1/plugin/no-auth-session",
            None,
        );
        respond(&mut rotate, "200 OK", &candidate("shared-token"));
        for _ in 0..2 {
            let (mut retry, retry_request) = request(&listener, deadline);
            assert_request(&retry_request, "GET", path, Some("shared-token"));
            respond(&mut retry, "200 OK", r#"{"total":0,"hits":[]}"#);
        }
        no_more_requests(&listener, Duration::from_millis(250));
    });
}

#[test]
fn authenticated_task_401_does_not_fall_back_to_no_auth() {
    run("authenticated", |listener, deadline, _| {
        let path = "/api/v1/plugin/jobs?dev_id=&status=0&offset=0&limit=20";
        let (mut tasks, tasks_request) = request(&listener, deadline);
        assert_request(&tasks_request, "GET", path, Some("stale-token"));
        respond(
            &mut tasks,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        no_more_requests(&listener, Duration::from_millis(500));
    });
}

#[test]
fn ambiguous_no_auth_rotation_is_attempted_only_once_per_credential_key() {
    run("ambiguous", |listener, deadline, _| {
        let path = "/api/v1/plugin/jobs?dev_id=&status=0&offset=0&limit=20";
        let (mut first, first_request) = request(&listener, deadline);
        let (mut second, second_request) = request(&listener, deadline);
        assert_request(&first_request, "GET", path, Some("stale-token"));
        assert_request(&second_request, "GET", path, Some("stale-token"));
        respond(
            &mut first,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        respond(
            &mut second,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        let (mut rotate, rotate_request) = request(&listener, deadline);
        assert_request(
            &rotate_request,
            "POST",
            "/api/v1/plugin/no-auth-session",
            None,
        );
        respond(
            &mut rotate,
            "409 Conflict",
            r#"{"error":"ambiguous_no_auth_tenant"}"#,
        );
        no_more_requests(&listener, Duration::from_millis(500));
    });
}

fn run_fence(mode: &'static str) {
    run(mode, move |listener, deadline, config| {
        let path = "/api/v1/plugin/jobs?dev_id=&status=0&offset=0&limit=20";
        let (mut tasks, tasks_request) = request(&listener, deadline);
        assert_request(&tasks_request, "GET", path, Some("stale-token"));
        respond(
            &mut tasks,
            "401 Unauthorized",
            r#"{"error":"invalid_auth_token"}"#,
        );
        let (mut rotate, rotate_request) = request(&listener, deadline);
        assert_request(
            &rotate_request,
            "POST",
            "/api/v1/plugin/no-auth-session",
            None,
        );
        std::fs::write(config.join("no-auth-post-entered"), b"entered").unwrap();
        if mode == "logout-race" {
            let (mut logout, logout_request) = request(&listener, deadline);
            assert_request(
                &logout_request,
                "DELETE",
                "/api/v1/plugin/session",
                Some("stale-token"),
            );
            respond(&mut logout, "204 No Content", "");
        }
        let release = config.join("no-auth-post-release");
        while !release.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(release.exists(), "probe did not release no-auth response");
        respond(&mut rotate, "200 OK", &candidate("race-candidate"));
        let (mut revoke, revoke_request) = request(&listener, deadline);
        assert_request(
            &revoke_request,
            "DELETE",
            "/api/v1/plugin/session",
            Some("race-candidate"),
        );
        respond(&mut revoke, "204 No Content", "");
        no_more_requests(&listener, Duration::from_millis(250));
    });
}

#[test]
fn concurrent_logout_fences_and_revokes_the_uncommitted_candidate() {
    run_fence("logout-race");
}

#[test]
fn concurrent_change_user_fences_and_revokes_the_uncommitted_candidate() {
    run_fence("change-race");
}

#[test]
fn concurrent_config_change_fences_and_revokes_the_uncommitted_candidate() {
    run_fence("config-race");
}

#[test]
fn persistence_preflight_failure_prevents_candidate_creation_and_retry() {
    run("persist-failure", |listener, _, _| {
        no_more_requests(&listener, Duration::from_millis(650));
    });
}

#[test]
fn post_preflight_persistence_failure_best_effort_revokes_the_candidate() {
    run(
        "post-preflight-persist-failure",
        |listener, deadline, config| {
            let (mut rotate, rotate_request) = request(&listener, deadline);
            assert_request(
                &rotate_request,
                "POST",
                "/api/v1/plugin/no-auth-session",
                None,
            );
            std::fs::remove_dir_all(&config).unwrap();
            std::fs::write(&config, b"block").unwrap();
            respond(&mut rotate, "200 OK", &candidate("persist-candidate"));
            let (mut revoke, revoke_request) = request(&listener, deadline);
            assert_request(
                &revoke_request,
                "DELETE",
                "/api/v1/plugin/session",
                Some("persist-candidate"),
            );
            respond(&mut revoke, "204 No Content", "");
            no_more_requests(&listener, Duration::from_millis(650));
        },
    );
}
