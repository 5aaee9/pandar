use std::{
    fs,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use super::{
    compiler::{build_plugin, compile_firmware_probe},
    firmware_mock::spawn_firmware_mock_hub,
};

const TIMEOUT: Duration = Duration::from_secs(45);

pub(super) struct FirmwareProbeOutput {
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) compiler: String,
}

pub(super) fn run_firmware_probe() -> FirmwareProbeOutput {
    let compiled = compile_firmware_probe();
    let built_library = build_plugin();
    let run_directory = tempfile::tempdir().expect("create firmware ABI run directory");
    let library = run_directory.path().join(
        built_library
            .file_name()
            .expect("built Studio plugin library has a file name"),
    );
    fs::copy(&built_library, &library).expect("copy Studio plugin library");
    let config_directory = run_directory.path().join("config");
    fs::create_dir(&config_directory).expect("create firmware ABI config directory");
    let hub = spawn_firmware_mock_hub(&config_directory);
    let mut child = Command::new(&compiled.executable)
        .arg(library)
        .arg(config_directory)
        .current_dir(run_directory.path())
        .env("PANDAR_PLUGIN_HUB_URL", &hub.url)
        .env("PANDAR_PLUGIN_FRONTEND_URL", "http://127.0.0.1:3000/pandar")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run firmware Studio ABI probe");
    let deadline = Instant::now() + TIMEOUT;
    let timed_out = loop {
        if child.try_wait().expect("poll firmware ABI probe").is_some() {
            break false;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            break true;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let output = child
        .wait_with_output()
        .expect("collect firmware ABI probe");
    let hub_result = hub.finish(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        !timed_out,
        "firmware ABI probe timed out\n{stdout}\n{stderr}"
    );
    assert!(
        output.status.success(),
        "firmware ABI probe failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    hub_result.expect("firmware mock Hub failed");
    FirmwareProbeOutput {
        stdout,
        stderr,
        compiler: compiled.compiler,
    }
}
