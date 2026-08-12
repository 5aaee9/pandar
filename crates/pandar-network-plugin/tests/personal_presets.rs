#![cfg(any(unix, windows))]

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[path = "personal_presets/pinned.rs"]
mod pinned;

struct CompiledProbe {
    executable: PathBuf,
    compiler: String,
    _directory: tempfile::TempDir,
}

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

fn plugin_path() -> PathBuf {
    let filename = if cfg!(target_os = "windows") {
        "pandar_network_plugin.dll"
    } else if cfg!(target_os = "macos") {
        "libpandar_network_plugin.dylib"
    } else {
        "libpandar_network_plugin.so"
    };
    target_dir().join("debug").join(filename)
}

fn build_plugin() -> PathBuf {
    let output = Command::new("cargo")
        .args(["build", "-p", "pandar-network-plugin"])
        .output()
        .expect("launch personal preset contract plugin build");
    assert!(
        output.status.success(),
        "plugin build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let path = plugin_path();
    assert!(
        path.is_file(),
        "plugin does not exist at {}",
        path.display()
    );
    path
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
            .expect("MSVC cl.exe is required for the personal preset ABI probe");
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
        panic!("a C++17 compiler is required for the personal preset ABI probe");
    }
}

fn compile_probe(series: &pandar_studio_profile::StudioAbiSeries) -> CompiledProbe {
    let directory = tempfile::tempdir().expect("create personal preset ABI compiler directory");
    let include = directory.path().join("studio-src");
    pinned::stage(series, &include);

    let executable = directory.path().join(if cfg!(windows) {
        format!("studio_personal_presets_{}.exe", series.id)
    } else {
        format!("studio_personal_presets_{}", series.id)
    });
    let object = executable.with_extension("obj");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/studio_personal_presets.cpp");
    let (mut command, compiler) = discovered_compiler();
    if cfg!(target_env = "msvc") {
        command
            .arg("/nologo")
            .arg("/std:c++17")
            .arg("/EHsc")
            .arg("/MD")
            .arg("/D_ITERATOR_DEBUG_LEVEL=0")
            .arg(format!("/I{}", include.display()))
            .arg(&fixture)
            .arg(format!("/Fe{}", executable.display()))
            .arg(format!("/Fo{}", object.display()));
    } else {
        command
            .arg("-std=c++17")
            .arg(format!("-I{}", include.display()))
            .arg(&fixture)
            .arg("-o")
            .arg(&executable);
        if cfg!(target_os = "linux") {
            command.args(["-pthread", "-ldl"]);
        }
    }
    let output = command.output().expect("launch C++17 compiler");
    assert!(
        output.status.success(),
        "personal preset fixture failed for {} with {compiler}\nstdout:\n{}\nstderr:\n{}",
        series.id,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    CompiledProbe {
        executable,
        compiler,
        _directory: directory,
    }
}

fn assert_series_contract(library: &Path, series: &pandar_studio_profile::StudioAbiSeries) {
    let probe = compile_probe(series);
    let run_directory = tempfile::tempdir().expect("create personal preset run directory");
    let config = run_directory.path().join("config");
    fs::create_dir(&config).unwrap();
    let output = Command::new(&probe.executable)
        .arg(library)
        .arg(&config)
        .current_dir(run_directory.path())
        .env_remove("PANDAR_PLUGIN_HUB_URL")
        .env_remove("APP_API_URL")
        .env_remove("PANDAR_PLUGIN_FRONTEND_URL")
        .env_remove("APP_BASE_URL")
        .env_remove("PANDAR_HUB_NO_AUTH")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run personal preset ABI probe");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "personal preset probe failed for {} with {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        series.id,
        probe.compiler
    );
    let result: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(
        result["ok"],
        serde_json::json!(true),
        "series {}",
        series.id
    );
    assert_eq!(
        result["contract_state"],
        serde_json::json!("handled_personal_presets")
    );
    assert_eq!(result["calls"], serde_json::json!(6));
    assert_eq!(result["callbacks_invoked"], serde_json::json!(0));
    assert_eq!(result["http_code"], serde_json::json!(403));
    assert!(
        stderr.is_empty(),
        "unexpected stderr for {}: {stderr}",
        series.id
    );
}

#[test]
fn all_pinned_studio_series_expose_the_handled_personal_preset_contract() {
    let library = build_plugin();
    let catalog = pandar_studio_profile::catalog();
    assert_eq!(
        catalog.abi_series.len(),
        6,
        "catalog series coverage changed"
    );
    for series in &catalog.abi_series {
        assert_series_contract(&library, series);
    }
}
