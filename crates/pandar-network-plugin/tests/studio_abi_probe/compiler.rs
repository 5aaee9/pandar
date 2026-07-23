use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

pub(super) struct CompiledProbe {
    pub(super) executable: PathBuf,
    pub(super) compiler: String,
    _directory: tempfile::TempDir,
}

pub(super) fn target_dir() -> PathBuf {
    if let Some(configured) = env::var_os("CARGO_TARGET_DIR")
        && !configured.is_empty()
    {
        let configured = PathBuf::from(configured);
        return if configured.is_absolute() {
            configured
        } else {
            env::current_dir()
                .expect("resolve current directory for CARGO_TARGET_DIR")
                .join(configured)
        };
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("network plugin is in the workspace crates directory")
        .join("target")
}

fn dynamic_library_path() -> PathBuf {
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_owned());
    let filename = if cfg!(target_os = "windows") {
        "pandar_network_plugin.dll"
    } else if cfg!(target_os = "macos") {
        "libpandar_network_plugin.dylib"
    } else {
        "libpandar_network_plugin.so"
    };
    target_dir().join(profile).join(filename)
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
            .expect("MSVC cl.exe is required to compile the Studio ABI probe");
        let compiler = tool.path().display().to_string();
        (tool.to_command(), compiler)
    }

    #[cfg(not(all(windows, target_env = "msvc")))]
    {
        for candidate in ["c++", "g++", "clang++"] {
            let output = Command::new(candidate).arg("--version").output();
            if output.is_ok_and(|output| output.status.success()) {
                return (Command::new(candidate), candidate.to_owned());
            }
        }
        panic!("a C++ compiler is required via CXX, c++, g++, or clang++");
    }
}

fn compile_fixture(mode_arg: &str, fixture_name: &str) -> CompiledProbe {
    let directory = tempfile::tempdir().expect("create Studio ABI compiler directory");
    let executable = directory.path().join(if cfg!(windows) {
        format!("studio_abi_probe_{mode_arg}.exe")
    } else {
        format!("studio_abi_probe_{mode_arg}")
    });
    let object = executable.with_extension("obj");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture_name);
    let (mut command, compiler) = discovered_compiler();

    if cfg!(target_env = "msvc") {
        command
            .arg("/nologo")
            .arg("/std:c++17")
            .arg("/EHsc")
            .arg("/MD")
            .arg("/D_ITERATOR_DEBUG_LEVEL=0")
            .arg(&fixture)
            .arg(format!("/Fe{}", executable.display()))
            .arg(format!("/Fo{}", object.display()))
            .arg("ws2_32.lib");
    } else {
        command
            .arg("-std=c++17")
            .arg(&fixture)
            .arg("-o")
            .arg(&executable);
        if cfg!(unix) {
            command.arg("-pthread");
        }
        if cfg!(target_os = "linux") {
            command.arg("-ldl");
        } else if cfg!(all(windows, target_env = "gnu")) {
            command.arg("-lws2_32");
        }
    }

    let output = command.output().unwrap_or_else(|error| {
        panic!("failed to launch explicit/discovered C++ compiler {compiler}: {error}")
    });
    assert!(
        output.status.success(),
        "failed to compile Studio ABI probe with {compiler}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    CompiledProbe {
        executable,
        compiler,
        _directory: directory,
    }
}

pub(super) fn compile_probe(mode_arg: &str) -> CompiledProbe {
    compile_fixture(mode_arg, "studio_abi_probe.cpp")
}

pub(super) fn compile_firmware_probe() -> CompiledProbe {
    compile_fixture("firmware", "firmware_abi_probe.cpp")
}

pub(super) fn compile_firmware_snapshot_claim_probe() -> CompiledProbe {
    compile_fixture(
        "firmware-snapshot-claim",
        "firmware_snapshot_claim_probe.cpp",
    )
}

pub(super) fn compile_disposition_probe() -> CompiledProbe {
    compile_fixture("dispositions", "studio_disposition_probe.cpp")
}

pub(super) fn build_plugin() -> PathBuf {
    let output = Command::new("cargo")
        .args(["build", "-p", "pandar-network-plugin"])
        .output()
        .expect("cargo build -p pandar-network-plugin is required before ABI probe");
    assert!(
        output.status.success(),
        "cargo build -p pandar-network-plugin failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let library = dynamic_library_path();
    assert!(
        library.exists(),
        "dynamic library does not exist at {}",
        library.display()
    );
    library
}
