use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

use super::pinned;

struct CompiledConsumer {
    executable: PathBuf,
    compiler: String,
    _directory: tempfile::TempDir,
}

static CONSUMER: OnceLock<CompiledConsumer> = OnceLock::new();

pub(super) fn run(status: &str) -> serde_json::Value {
    let consumer = CONSUMER.get_or_init(compile);
    let output = Command::new(&consumer.executable)
        .arg(status)
        .current_dir(
            consumer
                .executable
                .parent()
                .expect("projection consumer has a run directory"),
        )
        .output()
        .expect("run pinned Studio projection consumer");
    assert!(
        output.status.success(),
        "pinned Studio projection consumer failed (compiler {})\nstdout:\n{}\nstderr:\n{}",
        consumer.compiler,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("consumer emitted JSON")
}

fn compile() -> CompiledConsumer {
    let directory = tempfile::tempdir().expect("create projection consumer directory");
    let executable = directory.path().join(if cfg!(windows) {
        "studio_projection_consumer.exe"
    } else {
        "studio_projection_consumer"
    });
    let object = executable.with_extension("obj");
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().and_then(Path::parent).unwrap();
    pinned::stage(workspace, directory.path());
    let fixture = manifest.join("tests/fixtures/studio_projection_consumer.cpp");
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
    }
    let output = command.output().expect("launch C++17 compiler");
    assert!(
        output.status.success(),
        "projection consumer compile failed with {compiler}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    CompiledConsumer {
        executable,
        compiler,
        _directory: directory,
    }
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
            .expect("MSVC cl.exe is required for the projection consumer");
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
        panic!("a C++17 compiler is required for the projection consumer");
    }
}
