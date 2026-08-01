use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{boost::prepare_archive, http_probe::PrintSink};
use pandar_studio_profile::StudioAbiSeries;

const RUN_TIMEOUT: Duration = Duration::from_secs(20);

pub struct NativeReport {
    pub boost_sha256: String,
    pub compiler: String,
    pub passed_modes: Vec<String>,
    pub failures: Vec<String>,
}

struct CompileInput<'a> {
    studio_source: &'a Path,
    boost_includes: &'a [PathBuf],
    fixture: &'a Path,
    executable: &'a Path,
    check_types: bool,
    address_sanitizer: bool,
    abi_series: &'a StudioAbiSeries,
}

pub fn verify_native_contract(
    studio_source: &Path,
    plugin: &Path,
    boost_archive: &Path,
    modes: &[&str],
    check_types: bool,
    address_sanitizer: bool,
    abi_series: &StudioAbiSeries,
) -> Result<NativeReport, String> {
    if address_sanitizer && !cfg!(target_os = "linux") {
        return Err(
            "the address-sanitized FT contract is supported on Linux runners only".to_owned(),
        );
    }
    let boost = prepare_archive(boost_archive)?;
    let temp = tempfile::tempdir()
        .map_err(|error| format!("create native contract directory: {error}"))?;
    let fixture = fixture_path()?;
    let (compiler, compiler_name) = discovered_compiler()?;

    let mut failures = Vec::new();
    if check_types {
        let type_output = compile(
            compiler,
            &compiler_name,
            CompileInput {
                studio_source,
                boost_includes: &boost.include_roots,
                fixture: &fixture,
                executable: &temp.path().join(executable_name("studio_contract_types")),
                check_types: true,
                address_sanitizer,
                abi_series,
            },
        )?;
        if !type_output.status.success() {
            failures.push(format!(
                "PrintParams target layout check failed to compile with {}: {}",
                compiler_name,
                compact_diagnostic(&type_output)
            ));
        }
    }

    let (runtime_compiler, _) = discovered_compiler()?;
    let runtime = temp.path().join(executable_name("studio_contract_runtime"));
    let runtime_output = compile(
        runtime_compiler,
        &compiler_name,
        CompileInput {
            studio_source,
            boost_includes: &boost.include_roots,
            fixture: &fixture,
            executable: &runtime,
            check_types: false,
            address_sanitizer,
            abi_series,
        },
    )?;
    if !runtime_output.status.success() {
        return Err(format!(
            "compile pinned Studio native caller with {}: {}",
            compiler_name,
            compact_diagnostic(&runtime_output)
        ));
    }

    let mut passed_modes = Vec::new();
    let artifact = temp.path().join("contract-print.3mf");
    fs::write(&artifact, b"pinned Studio contract artifact")
        .map_err(|error| format!("write contract print artifact: {error}"))?;
    for mode in modes {
        let print_sink = if *mode == "print" {
            Some(PrintSink::spawn()?)
        } else {
            None
        };
        let output = match run_with_timeout(
            &runtime,
            plugin,
            mode,
            (*mode == "print").then_some(artifact.as_path()),
            print_sink.as_ref().map(|sink| sink.url.as_str()),
        ) {
            Ok(output) => output,
            Err(error) => {
                failures.push(error);
                if let Some(sink) = print_sink {
                    let _ = sink.finish();
                }
                continue;
            }
        };
        let print_observation = match print_sink.map(PrintSink::finish).transpose() {
            Ok(observation) => observation,
            Err(error) => {
                failures.push(error);
                None
            }
        };
        if output.status.success() {
            if let Some(request) = print_observation {
                let request = String::from_utf8_lossy(&request);
                let mut sentinels = vec![
                    "contract-printer",
                    "contract-task",
                    "713",
                    "[17,23]",
                    "contract-tail",
                ];
                if abi_series.capabilities.print_slicer_uid {
                    sentinels.push("contract-slicer-uid");
                }
                let observed = sentinels
                    .into_iter()
                    .all(|sentinel| request.contains(sentinel));
                if observed {
                    passed_modes.push((*mode).to_owned());
                } else {
                    failures.push(
                        "native print contract did not observe target PrintParams sentinels at the Hub boundary"
                            .to_owned(),
                    );
                }
            } else {
                passed_modes.push((*mode).to_owned());
            }
        } else {
            failures.push(format!(
                "native {mode} contract failed with {}: {}",
                output.status,
                compact_diagnostic(&output)
            ));
        }
    }

    Ok(NativeReport {
        boost_sha256: boost.sha256,
        compiler: compiler_name,
        passed_modes,
        failures,
    })
}

fn fixture_path() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "studio-abi-contract must remain under tools/".to_owned())?;
    let fixture =
        root.join("crates/pandar-network-plugin/tests/fixtures/studio_upstream_contract.cpp");
    if !fixture.is_file() {
        return Err(format!(
            "missing native contract fixture: {}",
            fixture.display()
        ));
    }
    Ok(fixture)
}

fn executable_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_owned()
    }
}

#[cfg(all(windows, target_env = "msvc"))]
fn discovered_compiler() -> Result<(Command, String), String> {
    let tool = cc::windows_registry::find_tool(std::env::consts::ARCH, "cl.exe")
        .ok_or_else(|| "MSVC cl.exe is required for the native Studio contract".to_owned())?;
    let name = tool.path().display().to_string();
    Ok((tool.to_command(), name))
}

#[cfg(not(all(windows, target_env = "msvc")))]
fn discovered_compiler() -> Result<(Command, String), String> {
    for candidate in ["c++", "g++", "clang++"] {
        if Command::new(candidate)
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
        {
            return Ok((Command::new(candidate), candidate.to_owned()));
        }
    }
    Err("a native C++17 compiler is required via c++, g++, or clang++".to_owned())
}

fn compile(
    mut compiler: Command,
    compiler_name: &str,
    input: CompileInput<'_>,
) -> Result<Output, String> {
    let plugin_headers = input
        .fixture
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| "resolve plugin source include path".to_owned())?
        .join("src");
    let studio_headers = input.studio_source.join("src");
    let profile_defines = [
        (
            input.abi_series.capabilities.filament_cloud,
            "PANDAR_STUDIO_FILAMENT_CLOUD",
        ),
        (
            input.abi_series.capabilities.print_svc_context,
            "PANDAR_STUDIO_PRINT_SVC_CONTEXT",
        ),
        (
            input.abi_series.capabilities.print_slicer_uid,
            "PANDAR_STUDIO_PRINT_SLICER_UID",
        ),
        (
            input.abi_series.capabilities.bind_model_argument,
            "PANDAR_STUDIO_BIND_MODEL_ARGUMENT",
        ),
        (
            input.abi_series.capabilities.ams_sync,
            "PANDAR_STUDIO_AMS_SYNC",
        ),
    ];
    if cfg!(all(windows, target_env = "msvc")) {
        compiler
            .arg("/nologo")
            .arg("/std:c++17")
            .arg("/EHsc")
            .arg("/MD")
            .arg("/D_ITERATOR_DEBUG_LEVEL=0")
            .arg("/DBOOST_ALL_NO_LIB")
            .arg(format!(
                "/DPANDAR_STUDIO_REPORTED_NETWORK_AGENT_VERSION=\"{}\"",
                input.abi_series.reported_network_agent_version
            ))
            .arg(format!("/I{}", studio_headers.display()))
            .arg(format!("/I{}", plugin_headers.display()));
        for include in input.boost_includes {
            compiler.arg(format!("/I{}", include.display()));
        }
        for (_, define) in profile_defines.iter().filter(|(enabled, _)| *enabled) {
            compiler.arg(format!("/D{define}"));
        }
        if input.check_types {
            compiler.arg("/DPANDAR_CONTRACT_CHECK_TYPES");
        }
        compiler
            .arg(input.fixture)
            .arg(format!("/Fe{}", input.executable.display()))
            .arg(format!(
                "/Fo{}",
                input.executable.with_extension("obj").display()
            ));
    } else {
        compiler
            .arg("-std=c++17")
            .arg("-DBOOST_ALL_NO_LIB")
            .arg(format!(
                "-DPANDAR_STUDIO_REPORTED_NETWORK_AGENT_VERSION=\"{}\"",
                input.abi_series.reported_network_agent_version
            ))
            .arg(format!("-I{}", studio_headers.display()))
            .arg(format!("-I{}", plugin_headers.display()));
        for include in input.boost_includes {
            compiler.arg(format!("-I{}", include.display()));
        }
        for (_, define) in profile_defines.iter().filter(|(enabled, _)| *enabled) {
            compiler.arg(format!("-D{define}"));
        }
        if input.address_sanitizer {
            compiler.args(["-fsanitize=address", "-fno-omit-frame-pointer", "-g"]);
        }
        if input.check_types {
            compiler.arg("-DPANDAR_CONTRACT_CHECK_TYPES");
        }
        compiler.arg(input.fixture).arg("-o").arg(input.executable);
        if cfg!(target_os = "linux") {
            compiler.arg("-ldl");
        }
    }
    compiler
        .output()
        .map_err(|error| format!("launch native contract compiler {compiler_name}: {error}"))
}

fn run_with_timeout(
    executable: &Path,
    plugin: &Path,
    mode: &str,
    artifact: Option<&Path>,
    hub_url: Option<&str>,
) -> Result<Output, String> {
    let mut command = Command::new(executable);
    command.arg(plugin).arg(mode);
    if let Some(artifact) = artifact {
        command.arg(artifact);
    }
    if let Some(hub_url) = hub_url {
        command
            .env("PANDAR_PLUGIN_HUB_URL", hub_url)
            .env("PANDAR_PLUGIN_FRONTEND_URL", "http://127.0.0.1:1");
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("run native {mode} contract: {error}"))?;
    let deadline = Instant::now() + RUN_TIMEOUT;
    loop {
        match child
            .try_wait()
            .map_err(|error| format!("poll native {mode} contract: {error}"))?
        {
            Some(_) => {
                return child
                    .wait_with_output()
                    .map_err(|error| format!("collect native {mode} contract: {error}"));
            }
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            None => {
                child
                    .kill()
                    .map_err(|error| format!("kill timed out native {mode} contract: {error}"))?;
                let output = child.wait_with_output().map_err(|error| {
                    format!("collect timed out native {mode} contract: {error}")
                })?;
                return Err(format!(
                    "native {mode} contract timed out after {RUN_TIMEOUT:?}: {}",
                    compact_diagnostic(&output)
                ));
            }
        }
    }
}

fn compact_diagnostic(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{} {}", stdout.trim(), stderr.trim());
    let compact = combined.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() > 1_200 {
        format!("{}...", compact.chars().take(1_200).collect::<String>())
    } else if compact.is_empty() {
        "no diagnostic output".to_owned()
    } else {
        compact
    }
}
