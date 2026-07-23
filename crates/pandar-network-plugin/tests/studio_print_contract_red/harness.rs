use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::OnceLock,
    thread,
    time::{Duration, Instant},
};

use crate::pinned;

#[path = "harness/mock_hub.rs"]
mod mock_hub;
use mock_hub::MockHub;
#[path = "harness/plugin.rs"]
mod plugin;
pub(super) use plugin::PLUGIN_OVERRIDE_ENV;
use plugin::{SelectedLibrary, select_library};

const ARTIFACT_BYTES: &[u8] = b"studio print contract artifact bytes";
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
pub(super) const DIAGNOSTIC_SECRET: &str = "/private/diagnostic-secret-token@198.51.100.91";

pub(super) struct ProbeEvidence {
    pub(super) output: serde_json::Value,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) trace: String,
    pub(super) requests: Vec<String>,
    pub(super) artifact_path: String,
    pub(super) config_path: String,
    pub(super) plugin_path: String,
    pub(super) plugin_sha256: String,
    pub(super) plugin_source: &'static str,
}

impl ProbeEvidence {
    pub(super) fn assert_excludes(&self, value: &str) {
        let requests = self.requests.join("\n");
        let output = self.output.to_string();
        for (surface, content) in [
            ("stdout", self.stdout.as_str()),
            ("stderr", self.stderr.as_str()),
            ("requests", requests.as_str()),
            ("probe output", output.as_str()),
        ] {
            assert!(!content.contains(value), "{surface} leaked {value}");
        }
    }
}

struct CompiledProbe {
    executable: PathBuf,
    compiler: String,
    _directory: tempfile::TempDir,
}

static LIBRARY: OnceLock<SelectedLibrary> = OnceLock::new();
static PROBE: OnceLock<CompiledProbe> = OnceLock::new();

fn target_dir() -> PathBuf {
    if let Some(configured) = env::var_os("CARGO_TARGET_DIR")
        && !configured.is_empty()
    {
        let configured = PathBuf::from(configured);
        return if configured.is_absolute() {
            configured
        } else {
            env::current_dir().unwrap().join(configured)
        };
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .join("target")
}

fn dynamic_library_path() -> PathBuf {
    let filename = if cfg!(target_os = "windows") {
        "pandar_network_plugin.dll"
    } else if cfg!(target_os = "macos") {
        "libpandar_network_plugin.dylib"
    } else {
        "libpandar_network_plugin.so"
    };
    target_dir().join("debug").join(filename)
}

fn selected_library() -> &'static SelectedLibrary {
    LIBRARY.get_or_init(|| {
        let configured = env::var_os(PLUGIN_OVERRIDE_ENV);
        let selected = select_library(configured.as_deref(), || {
            let output = Command::new("cargo")
                .args(["build", "-p", "pandar-network-plugin"])
                .output()
                .map_err(|error| {
                    format!("launch plugin build for compiled Studio ABI contract: {error}")
                })?;
            if !output.status.success() {
                return Err(format!(
                    "plugin build failed\nstdout:\n{}\nstderr:\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            Ok(dynamic_library_path())
        })
        .unwrap_or_else(|error| panic!("select Studio print contract plugin: {error}"));
        eprintln!(
            "studio_print_contract_plugin source={} path={} sha256={}",
            selected.source,
            selected.identity.path.display(),
            selected.identity.sha256
        );
        selected
    })
}

fn discovered_compiler() -> (Command, String) {
    if let Ok(cxx) = env::var("CXX")
        && !cxx.trim().is_empty()
    {
        return (Command::new(&cxx), cxx);
    }

    #[cfg(all(windows, target_env = "msvc"))]
    {
        let tool = cc::windows_registry::find_tool(env::consts::ARCH, "cl.exe")
            .expect("MSVC cl.exe is required for the Studio print contract probe");
        let compiler = tool.path().display().to_string();
        (tool.to_command(), compiler)
    }

    #[cfg(not(all(windows, target_env = "msvc")))]
    {
        for candidate in ["c++", "g++", "clang++"] {
            if Command::new(candidate)
                .arg("--version")
                .output()
                .is_ok_and(|output| output.status.success())
            {
                return (Command::new(candidate), candidate.to_owned());
            }
        }
        panic!("a C++17 compiler is required for the Studio print contract probe");
    }
}

fn compiled_probe() -> &'static CompiledProbe {
    PROBE.get_or_init(|| {
        let directory =
            tempfile::tempdir().expect("create Studio print contract compiler directory");
        let executable = directory.path().join(if cfg!(windows) {
            "studio_print_contract_red.exe"
        } else {
            "studio_print_contract_red"
        });
        let object = executable.with_extension("obj");
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/studio_print_contract_red.cpp");
        let manifest_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace = manifest_directory.parent().and_then(Path::parent).unwrap();
        pinned::stage(workspace, directory.path());
        let (mut command, compiler) = discovered_compiler();
        if cfg!(target_env = "msvc") {
            command
                .arg("/nologo")
                .arg("/std:c++17")
                .arg("/EHsc")
                .arg("/MD")
                .arg("/D_ITERATOR_DEBUG_LEVEL=0")
                .arg(format!("/I{}", directory.path().display()))
                .arg(&fixture)
                .arg(format!("/Fe{}", executable.display()))
                .arg(format!("/Fo{}", object.display()));
        } else {
            command
                .arg("-std=c++17")
                .arg(format!("-I{}", directory.path().display()))
                .arg(&fixture)
                .arg("-o")
                .arg(&executable);
            if cfg!(target_os = "linux") {
                command.arg("-ldl");
            }
        }
        let output = command.output().expect("launch C++17 compiler");
        assert!(
            output.status.success(),
            "fixture compile failed with {compiler}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        CompiledProbe {
            executable,
            compiler,
            _directory: directory,
        }
    })
}

enum ChildRun {
    Exited(Output),
    TimedOut(Output),
}

fn wait_for_child(mut child: Child) -> ChildRun {
    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        match child.try_wait().expect("poll Studio print contract probe") {
            Some(_) => return ChildRun::Exited(child.wait_with_output().unwrap()),
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            None => {
                let _ = child.kill();
                return ChildRun::TimedOut(child.wait_with_output().unwrap());
            }
        }
    }
}

pub(super) fn run_probe(mode: &str, case: &str) -> ProbeEvidence {
    let run_directory = tempfile::tempdir().expect("create Studio print contract run directory");
    let hub = MockHub::spawn(case, run_directory.path());
    let artifact = run_directory.path().join("contract-artifact.3mf");
    let config = run_directory.path().join("contract-private-config.3mf");
    if case == "cancel_upload" {
        fs::write(&artifact, vec![b'x'; 3 * 64 * 1024]).unwrap();
    } else {
        fs::write(&artifact, ARTIFACT_BYTES).unwrap();
    }
    let mut archive = zip::ZipWriter::new(fs::File::create(&config).unwrap());
    archive
        .start_file(
            "Metadata/slice_info.config",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    let config_contents: &[u8] = if case == "invalid_config_xml" {
        br#"<config><plate><diagnostic-secret-token-198.51.100.91></wrong>"#
    } else {
        br#"<config><plate><metadata key="index" value="7"/></plate></config>"#
    };
    archive.write_all(config_contents).unwrap();
    archive.finish().unwrap();
    let compiled = compiled_probe();
    let hub_url = if case == "trailing_slash_hub" {
        format!("{}/", hub.url)
    } else {
        hub.url.clone()
    };
    if case == "model_task_destroy_no_auth_recovery" {
        fs::write(
            run_directory.path().join("pandar-plugin-login.json"),
            format!(
                r#"{{"hub_url":"{}","token":"stale-token","session_kind":"no_auth","profile":{{"user_id":"stale-user","user_name":"Stale User","tenant_id":"contract-tenant","tenant_name":"Contract Tenant"}}}}"#,
                hub.url
            ),
        )
        .unwrap();
    }
    let library = selected_library();
    let child = Command::new(&compiled.executable)
        .arg(&library.identity.path)
        .arg(mode)
        .arg(case)
        .arg(&artifact)
        .arg(run_directory.path())
        .arg(&config)
        .current_dir(run_directory.path())
        .env("PANDAR_PLUGIN_HUB_URL", hub_url)
        .env_remove("PANDAR_HUB_NO_AUTH")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run compiled Studio print contract probe");
    let (output, timed_out) = match wait_for_child(child) {
        ChildRun::Exited(output) => (output, false),
        ChildRun::TimedOut(output) => (output, true),
    };
    let requests = hub.finish();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let trace = fs::read_to_string(run_directory.path().join("pandar-network-plugin.trace.log"))
        .unwrap_or_default();
    assert!(
        !timed_out && output.status.success(),
        "Studio print contract probe failed (compiler {})\nstdout:\n{stdout}\nstderr:\n{stderr}",
        compiled.compiler
    );
    let json_line = stdout
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with('{'))
        .expect("probe emitted JSON");
    ProbeEvidence {
        output: serde_json::from_str(json_line).expect("parse probe JSON"),
        stdout,
        stderr,
        trace,
        requests,
        artifact_path: artifact.display().to_string(),
        config_path: config.display().to_string(),
        plugin_path: library.identity.path.display().to_string(),
        plugin_sha256: library.identity.sha256.clone(),
        plugin_source: library.source,
    }
}

pub(super) fn print_requests(evidence: &ProbeEvidence) -> Vec<&str> {
    evidence
        .requests
        .iter()
        .filter(|request| request.starts_with("POST /api/v1/plugin/prints "))
        .map(String::as_str)
        .collect()
}
