mod support;

use std::{
    env, fs,
    io::Write,
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
use support::{
    assert_multipart_file_part, assert_multipart_print_request, read_http_request_with_timeout,
    request_body,
};

const MOCK_HUB_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

fn target_dir() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop();
    dir.pop();
    dir.join("target")
}

fn dynamic_library_path() -> PathBuf {
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let filename = if cfg!(target_os = "windows") {
        "pandar_network_plugin.dll"
    } else if cfg!(target_os = "macos") {
        "libpandar_network_plugin.dylib"
    } else {
        "libpandar_network_plugin.so"
    };
    target_dir().join(profile).join(filename)
}

fn find_cxx() -> Option<String> {
    if let Ok(cxx) = env::var("CXX")
        && !cxx.trim().is_empty()
    {
        return Some(cxx);
    }
    ["c++", "g++", "clang++"]
        .into_iter()
        .find(|candidate| {
            Command::new(candidate)
                .arg("--version")
                .output()
                .is_ok_and(|output| output.status.success())
        })
        .map(str::to_owned)
}

fn compile_probe(mode_arg: &str) -> Option<PathBuf> {
    if !(cfg!(unix) || cfg!(windows)) {
        println!("skipping studio ABI probe: platform dynamic loading is not supported");
        return None;
    }
    let Some(cxx) = find_cxx() else {
        println!("skipping studio ABI probe: no C++ compiler found via CXX, c++, g++, or clang++");
        return None;
    };

    let output = target_dir()
        .join("studio-abi-probe")
        .join(if cfg!(windows) {
            format!("studio_abi_probe_{}_{mode_arg}.exe", std::process::id())
        } else {
            format!("studio_abi_probe_{}_{mode_arg}", std::process::id())
        });
    fs::create_dir_all(output.parent().unwrap()).unwrap();

    let mut command = Command::new(cxx);
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/studio_abi_probe.cpp");
    command
        .arg("-std=c++17")
        .arg(fixture)
        .arg("-o")
        .arg(&output);
    if cfg!(target_os = "linux") {
        command.arg("-ldl");
    }

    let result = command.output().unwrap();
    if !result.status.success() {
        panic!(
            "failed to compile studio ABI probe\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    Some(output)
}

fn build_plugin() -> PathBuf {
    let status = Command::new("cargo")
        .args(["build", "-p", "pandar-network-plugin"])
        .status()
        .expect("cargo build -p pandar-network-plugin is required before ABI probe");
    assert!(
        status.success(),
        "cargo build -p pandar-network-plugin failed"
    );
    let library = dynamic_library_path();
    assert!(
        library.exists(),
        "dynamic library does not exist at {}",
        library.display()
    );
    library
}

#[derive(Clone, Copy)]
enum MockMode {
    Success,
    Failure,
}

struct MockHub {
    url: String,
    handle: thread::JoinHandle<()>,
}

fn accept_with_timeout(listener: &TcpListener) -> TcpStream {
    let deadline = Instant::now() + MOCK_HUB_TIMEOUT;
    loop {
        match listener.accept() {
            Ok((stream, _)) => return stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "timed out waiting for mock hub request"
                );
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("failed accepting mock hub request: {error}"),
        }
    }
}

fn write_response(stream: &mut std::net::TcpStream, status: &str, body: &str) {
    let response = format!(
        "{status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).unwrap();
}

fn assert_request(request: &str, method: &str, path: &str, bearer: bool) {
    assert!(
        request.starts_with(&format!("{method} {path} HTTP/1.1\r\n")),
        "unexpected request line: {request}"
    );
    if bearer {
        assert!(
            request.contains("authorization: Bearer probe-token"),
            "missing bearer auth: {request}"
        );
    }
}

fn spawn_mock_hub(mode: MockMode, artifact: Vec<u8>) -> MockHub {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());

    let handle = thread::spawn(move || match mode {
        MockMode::Success => {
            let expected = [
                ("POST", "/api/v1/plugin/login-tickets/exchange", false),
                ("GET", "/api/v1/plugin/printers", true),
                ("GET", "/api/v1/plugin/jobs", true),
                ("POST", "/api/v1/plugin/prints", true),
                ("POST", "/api/v1/plugin/printers/printer-1/operations", true),
            ];
            for (index, (method, path, bearer)) in expected.into_iter().enumerate() {
                let mut stream = accept_with_timeout(&listener);
                stream.set_read_timeout(Some(MOCK_HUB_TIMEOUT)).unwrap();
                stream.set_write_timeout(Some(MOCK_HUB_TIMEOUT)).unwrap();
                let request = read_http_request_with_timeout(&mut stream, Some(MOCK_HUB_TIMEOUT));
                assert_request(&request, method, path, bearer);
                match index {
                    0 => write_response(
                        &mut stream,
                        "HTTP/1.1 200 OK",
                        r#"{"token":"probe-token","profile":{"token":"probe-token","user_id":"probe-user","user_name":"Probe User","tenant_id":"tenant-1","tenant_name":"Tenant"}}"#,
                    ),
                    1 => write_response(
                        &mut stream,
                        "HTTP/1.1 200 OK",
                        r#"{"devices":[{"dev_id":"printer-1","name":"Probe Printer"}]}"#,
                    ),
                    2 => write_response(&mut stream, "HTTP/1.1 200 OK", r#"{"tasks":[]}"#),
                    3 => {
                        let body = request_body(&request);
                        assert_multipart_print_request(&request);
                        assert!(
                            body.contains(r#"name="printer_id""#),
                            "bad print body: {body}"
                        );
                        assert!(
                            body.contains(r#"name="filename""#),
                            "bad print filename: {body}"
                        );
                        assert_multipart_file_part(&request, "probe.3mf", &artifact);
                        write_response(&mut stream, "HTTP/1.1 200 OK", r#"{"job_id":"job-1"}"#);
                    }
                    4 => {
                        assert_eq!(
                            serde_json::from_str::<serde_json::Value>(request_body(&request))
                                .unwrap(),
                            serde_json::json!({"action":"home","axes":["x"]})
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
                    _ => unreachable!(),
                }
            }
        }
        MockMode::Failure => {
            let expected = [
                ("POST", "/api/v1/plugin/login-tickets/exchange", false),
                ("GET", "/api/v1/plugin/printers", true),
                ("POST", "/api/v1/plugin/prints", true),
            ];
            for (index, (method, path, bearer)) in expected.into_iter().enumerate() {
                let mut stream = accept_with_timeout(&listener);
                stream.set_read_timeout(Some(MOCK_HUB_TIMEOUT)).unwrap();
                stream.set_write_timeout(Some(MOCK_HUB_TIMEOUT)).unwrap();
                let request = read_http_request_with_timeout(&mut stream, Some(MOCK_HUB_TIMEOUT));
                assert_request(&request, method, path, bearer);
                match index {
                    0 => write_response(
                        &mut stream,
                        "HTTP/1.1 401 Unauthorized",
                        r#"{"error":"raw-ticket-message","ticket":"secret"}"#,
                    ),
                    1 => write_response(
                        &mut stream,
                        "HTTP/1.1 401 Unauthorized",
                        r#"{"error":"raw-auth-message","token":"secret"}"#,
                    ),
                    2 => write_response(
                        &mut stream,
                        "HTTP/1.1 403 Forbidden",
                        r#"{"error":"raw-forbidden-message","path":"/tmp/secret.3mf"}"#,
                    ),
                    _ => unreachable!(),
                }
            }
        }
    });
    MockHub { url, handle }
}

enum ProbeRun {
    Exited(Output),
    TimedOut(Output),
}

fn wait_for_probe(mut command: Command) -> ProbeRun {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run studio ABI probe");
    let deadline = Instant::now() + PROBE_TIMEOUT;

    loop {
        match child.try_wait().expect("poll studio ABI probe") {
            Some(_) => {
                return ProbeRun::Exited(
                    child
                        .wait_with_output()
                        .expect("collect studio ABI probe output"),
                );
            }
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            None => {
                let _ = child.kill();
                let output = child
                    .wait_with_output()
                    .expect("collect timed out studio ABI probe output");
                return ProbeRun::TimedOut(output);
            }
        }
    }
}

fn run_probe(mode: MockMode, mode_arg: &str) -> Option<(String, String)> {
    let probe = compile_probe(mode_arg)?;
    let library = build_plugin();
    let artifact = target_dir().join(format!(
        "studio-abi-probe-{}-{mode_arg}.3mf",
        std::process::id()
    ));
    let artifact_bytes = b"probe artifact bytes".to_vec();
    fs::write(&artifact, &artifact_bytes).unwrap();
    let hub = spawn_mock_hub(mode, artifact_bytes);

    let mut command = Command::new(probe);
    command
        .arg(library)
        .arg(&artifact)
        .arg(mode_arg)
        .env("PANDAR_PLUGIN_HUB_URL", &hub.url)
        .env("PANDAR_PLUGIN_FRONTEND_URL", "http://127.0.0.1:3000/pandar");
    let probe_run = wait_for_probe(command);
    fs::remove_file(&artifact)
        .unwrap_or_else(|error| panic!("remove probe artifact {}: {error}", artifact.display()));
    let hub_result = hub.handle.join();

    let output = match probe_run {
        ProbeRun::Exited(output) => output,
        ProbeRun::TimedOut(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!(
                "timed out running studio ABI probe after {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
                PROBE_TIMEOUT
            );
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "studio ABI probe failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    if let Err(error) = hub_result {
        panic!(
            "mock hub thread panicked during ABI probe\nstdout:\n{stdout}\nstderr:\n{stderr}\npanic: {error:?}"
        );
    }
    Some((stdout, stderr))
}

fn assert_json_field(output: &str, field: &str, value: &str) {
    assert!(
        output.contains(&format!(r#""{field}":{value}"#)),
        "probe output missing {field}={value}: {output}"
    );
}

#[test]
fn probe_exercises_studio_abi_success_path() {
    let Some((stdout, stderr)) = run_probe(MockMode::Success, "success") else {
        return;
    };

    assert!(stderr.is_empty(), "probe stderr was not empty: {stderr}");
    assert_json_field(&stdout, "ok", "true");
    assert!(stdout.contains(r#""host":"http://127.0.0.1:3000/pandar/""#));
    assert!(stdout.contains("studio_userlogin"));
    assert!(stdout.contains("studio_useroffline"));
    assert_json_field(&stdout, "printer_rc", "0");
    assert_json_field(&stdout, "tasks_rc", "0");
    assert_json_field(&stdout, "print_rc", "0");
    assert_json_field(&stdout, "ft_abi_version", "1");
    assert_json_field(&stdout, "ft_start_connect_rc", "0");
    assert_json_field(&stdout, "ft_sync_rc", "-3");
    assert_json_field(&stdout, "ft_start_job_rc", "0");
    assert_json_field(&stdout, "ft_job_result_ec", "-3");
    assert_json_field(&stdout, "ft_cancel_rc", "0");
}

#[test]
fn probe_redacts_failed_hub_responses_through_abi() {
    let Some((stdout, stderr)) = run_probe(MockMode::Failure, "failure") else {
        return;
    };
    let combined = format!("{stdout}\n{stderr}");

    assert!(
        !combined.contains("secret"),
        "probe leaked secret: {combined}"
    );
    assert!(
        !combined.contains("/tmp/secret.3mf"),
        "probe leaked path: {combined}"
    );
    assert!(combined.contains("invalid_plugin_ticket"));
    assert!(combined.contains("invalid_auth_token"));
    assert!(combined.contains("plugin_forbidden"));
}
