use std::{fs, net::TcpListener, process::Command};

use super::compiler::{build_plugin, compile_disposition_probe};

pub(super) struct DispositionProbeOutput {
    pub(super) stdout: String,
    #[allow(dead_code)]
    pub(super) stderr: String,
    pub(super) compiler: String,
}

pub(super) fn run_disposition_probe() -> DispositionProbeOutput {
    let compiled = compile_disposition_probe();
    let built_library = build_plugin();
    let run_directory = tempfile::tempdir().expect("create disposition ABI run directory");
    let library = run_directory.path().join(
        built_library
            .file_name()
            .expect("built Studio plugin library has a file name"),
    );
    fs::copy(&built_library, &library).expect("copy Studio plugin library into run directory");
    let config_directory = run_directory.path().join("config");
    fs::create_dir(&config_directory).expect("create disposition ABI config directory");
    let hub_listener = TcpListener::bind("127.0.0.1:0").expect("bind disposition ABI Hub listener");
    let hub_url = format!(
        "http://{}",
        hub_listener
            .local_addr()
            .expect("read disposition ABI Hub listener address")
    );

    let output = Command::new(&compiled.executable)
        .arg(library)
        .arg(config_directory)
        .current_dir(run_directory.path())
        .env("PANDAR_PLUGIN_HUB_URL", hub_url)
        .env_remove("APP_API_URL")
        .env_remove("PANDAR_PLUGIN_FRONTEND_URL")
        .env_remove("APP_BASE_URL")
        .output()
        .expect("run Studio disposition ABI probe");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "Studio disposition ABI probe failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    DispositionProbeOutput {
        stdout,
        stderr,
        compiler: compiled.compiler,
    }
}
