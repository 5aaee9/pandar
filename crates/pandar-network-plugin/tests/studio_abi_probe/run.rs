use std::{
    any::Any,
    fs,
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use super::{
    compiler::{build_plugin, compile_probe},
    mock_hub::{MockMode, spawn_mock_hub},
};

const PROBE_TIMEOUT: Duration = Duration::from_secs(25);

enum ProbeRun {
    Exited(Output),
    TimedOut(Output),
}

pub(super) struct ProbeOutput {
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) compiler: String,
}

fn panic_message(error: Box<dyn Any + Send>) -> String {
    if let Some(message) = error.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = error.downcast_ref::<&str>() {
        (*message).to_owned()
    } else {
        "non-string panic payload".to_owned()
    }
}

fn spawn_probe(mut command: Command) -> Child {
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run Studio ABI probe")
}

fn wait_for_probe(mut child: Child, deadline: Instant) -> ProbeRun {
    loop {
        match child.try_wait().expect("poll Studio ABI probe") {
            Some(_) => {
                return ProbeRun::Exited(
                    child
                        .wait_with_output()
                        .expect("collect Studio ABI probe output"),
                );
            }
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            None => {
                let _ = child.kill();
                return ProbeRun::TimedOut(
                    child
                        .wait_with_output()
                        .expect("collect timed out Studio ABI probe output"),
                );
            }
        }
    }
}

pub(super) fn run_probe(mode: MockMode, mode_arg: &str) -> ProbeOutput {
    let compiled = compile_probe(mode_arg);
    let built_library = build_plugin();
    let run_directory = tempfile::tempdir().expect("create Studio ABI run directory");
    let library = run_directory.path().join(
        built_library
            .file_name()
            .expect("built Studio plugin library has a file name"),
    );
    fs::copy(&built_library, &library).expect("copy Studio plugin library into run directory");
    let config_directory = run_directory.path().join("config");
    fs::create_dir(&config_directory).expect("create owned Studio ABI config directory");
    let artifact = run_directory.path().join("probe.3mf");
    let artifact_bytes = b"probe artifact bytes".to_vec();
    fs::write(&artifact, &artifact_bytes).expect("write Studio ABI artifact");
    let hub = spawn_mock_hub(mode, artifact_bytes);

    let mut command = Command::new(&compiled.executable);
    command
        .arg(library)
        .arg(&artifact)
        .arg(mode_arg)
        .arg(&config_directory)
        .env("PANDAR_PLUGIN_HUB_URL", &hub.url)
        .env("PANDAR_PLUGIN_FRONTEND_URL", "http://127.0.0.1:3000/pandar");
    let child = spawn_probe(command);
    let deadline = Instant::now() + PROBE_TIMEOUT;
    hub.start(deadline);
    let probe_run = wait_for_probe(child, deadline);
    let hub_result = hub.finish();

    let (output, timed_out) = match probe_run {
        ProbeRun::Exited(output) => (output, false),
        ProbeRun::TimedOut(output) => (output, true),
    };
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let hub_failure = hub_result.err().map(panic_message);
    let hub_diagnostic = hub_failure
        .as_deref()
        .map_or_else(String::new, |error| format!("\nmock Hub failure: {error}"));
    assert!(
        !timed_out,
        "timed out running Studio ABI probe after {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}{hub_diagnostic}",
        PROBE_TIMEOUT
    );
    assert!(
        output.status.success(),
        "Studio ABI probe failed\nstdout:\n{stdout}\nstderr:\n{stderr}{hub_diagnostic}"
    );
    if let Some(error) = hub_failure {
        panic!(
            "mock Hub thread panicked during Studio ABI probe\nstdout:\n{stdout}\nstderr:\n{stderr}\npanic: {error}"
        );
    }

    ProbeOutput {
        stdout,
        stderr,
        compiler: compiled.compiler,
    }
}
