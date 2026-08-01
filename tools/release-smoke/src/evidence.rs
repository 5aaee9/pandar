use std::{
    env,
    fmt::Write,
    path::{Path, PathBuf},
    process::Command,
};

use crate::host::NativeTarget;

pub(crate) struct EvidenceInput<'a> {
    pub target: NativeTarget,
    pub studio_profile: &'a str,
    pub archive_sha256: &'a str,
    pub plugin_sha256: &'a str,
    pub source_sha256: &'a str,
    pub network_symbols: usize,
    pub file_transfer_symbols: usize,
    pub plugin_inspector: &'a str,
    pub source_inspector: &'a str,
    pub plugin: &'a Path,
    pub source_sentinel: &'a str,
}

pub(crate) fn collect_evidence(input: EvidenceInput<'_>) -> Result<String, String> {
    let rust = rust_toolchain()?;
    let cxx = cxx_toolchain(input.target)?;
    let runtime = runtime_abi(input.target, input.plugin)?;
    let mut report = String::new();
    for (name, value) in [
        ("release_smoke_status", "passed".to_owned()),
        ("target_label", input.target.label().to_owned()),
        ("studio_profile", input.studio_profile.to_owned()),
        ("host_os", env::consts::OS.to_owned()),
        ("host_arch", env::consts::ARCH.to_owned()),
        ("archive_sha256", input.archive_sha256.to_owned()),
        ("plugin_sha256", input.plugin_sha256.to_owned()),
        ("source_sha256", input.source_sha256.to_owned()),
        ("network_symbols", input.network_symbols.to_string()),
        (
            "file_transfer_symbols",
            input.file_transfer_symbols.to_string(),
        ),
        (
            "plugin_exports",
            (input.network_symbols + input.file_transfer_symbols).to_string(),
        ),
        ("export_inspector", input.plugin_inspector.to_owned()),
        ("source_export_inspector", input.source_inspector.to_owned()),
        ("source_sentinel_export", input.source_sentinel.to_owned()),
        ("source_bambu_exports", "0".to_owned()),
        ("host_rust_toolchain", rust.version),
        ("host_rust_target", rust.host),
        ("host_cxx_toolchain", cxx),
        ("host_runtime_abi", runtime),
        ("packaged_cli", "native-executed".to_owned()),
        ("packaged_plugin_probe", "native-executed".to_owned()),
        ("packaged_source_contract", "native-inspected".to_owned()),
    ] {
        writeln!(report, "{name}={}", one_line(&value)).expect("write String evidence");
    }
    Ok(report.trim_end().to_owned())
}

struct RustToolchain {
    version: String,
    host: String,
}

fn rust_toolchain() -> Result<RustToolchain, String> {
    let output = Command::new("rustc")
        .args(["--version", "--verbose"])
        .output()
        .map_err(|error| format!("run rustc --version --verbose: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "rustc --version --verbose exited with {}",
            output.status
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout
        .lines()
        .find(|line| line.starts_with("rustc "))
        .ok_or_else(|| "rustc version output omitted the release line".to_owned())?;
    let host = stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or_else(|| "rustc version output omitted the host target".to_owned())?;
    Ok(RustToolchain {
        version: version.to_owned(),
        host: host.to_owned(),
    })
}

fn cxx_toolchain(target: NativeTarget) -> Result<String, String> {
    let mut candidates = Vec::new();
    if let Some(cxx) = env::var_os("CXX") {
        candidates.push((PathBuf::from(cxx), cxx_version_arg(target)));
    }
    match target {
        NativeTarget::LinuxAmd64 => {
            for program in ["c++", "g++", "clang++"] {
                candidates.push((PathBuf::from(program), "--version"));
            }
        }
        NativeTarget::MacosAmd64 | NativeTarget::MacosArm64 => {
            for program in ["c++", "clang++"] {
                candidates.push((PathBuf::from(program), "--version"));
            }
        }
        NativeTarget::WindowsAmd64 => {
            candidates.push((PathBuf::from("cl.exe"), "/Bv"));
            candidates.push((PathBuf::from("clang-cl.exe"), "--version"));
            if let Some(cl) = visual_studio_cl() {
                candidates.push((cl, "/Bv"));
            }
        }
    }
    for (program, argument) in candidates {
        let Ok(output) = Command::new(&program).arg(argument).output() else {
            continue;
        };
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if let Some(version) = combined.lines().find(|line| !line.trim().is_empty()) {
            let name = program
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("cxx");
            return Ok(format!("{name}: {}", version.trim()));
        }
    }
    Err("no native C++ compiler version command succeeded".to_owned())
}

fn cxx_version_arg(target: NativeTarget) -> &'static str {
    match target {
        NativeTarget::LinuxAmd64 | NativeTarget::MacosAmd64 | NativeTarget::MacosArm64 => {
            "--version"
        }
        NativeTarget::WindowsAmd64 => "/Bv",
    }
}

fn runtime_abi(target: NativeTarget, plugin: &Path) -> Result<String, String> {
    match target {
        NativeTarget::WindowsAmd64 => windows_runtime_abi(plugin),
        NativeTarget::MacosAmd64 | NativeTarget::MacosArm64 => macos_runtime_abi(plugin),
        NativeTarget::LinuxAmd64 => {
            let version = Command::new("ldd")
                .arg("--version")
                .output()
                .map_err(|error| format!("run ldd --version: {error}"))?;
            if !version.status.success() {
                return Err(format!("ldd --version exited with {}", version.status));
            }
            let combined = format!(
                "{}\n{}",
                String::from_utf8_lossy(&version.stdout),
                String::from_utf8_lossy(&version.stderr)
            );
            let version = combined
                .lines()
                .find(|line| !line.trim().is_empty())
                .map(|line| line.trim().to_owned())
                .ok_or_else(|| "ldd --version produced no runtime metadata".to_owned())?;
            let dependencies = Command::new("ldd").arg(plugin).output().map_err(|error| {
                format!("inspect packaged plugin runtime dependencies: {error}")
            })?;
            if !dependencies.status.success() {
                return Err(format!(
                    "packaged plugin ldd inspection exited with {}",
                    dependencies.status
                ));
            }
            let mut libraries = String::from_utf8_lossy(&dependencies.stdout)
                .lines()
                .filter_map(|line| line.split_whitespace().next())
                .filter(|name| name.ends_with(".so") || name.contains(".so."))
                .map(str::to_owned)
                .collect::<Vec<_>>();
            libraries.sort();
            libraries.dedup();
            if libraries.is_empty() {
                return Err(
                    "packaged plugin ldd inspection found no shared runtime libraries".to_owned(),
                );
            }
            Ok(format!("{version}; imports={}", libraries.join(",")))
        }
    }
}

fn macos_runtime_abi(plugin: &Path) -> Result<String, String> {
    let version = Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .map_err(|error| format!("run sw_vers -productVersion: {error}"))?;
    if !version.status.success() {
        return Err(format!(
            "sw_vers -productVersion exited with {}",
            version.status
        ));
    }
    let version = String::from_utf8_lossy(&version.stdout).trim().to_owned();
    if version.is_empty() {
        return Err("sw_vers -productVersion produced no runtime metadata".to_owned());
    }

    let dependencies = Command::new("otool")
        .arg("-L")
        .arg(plugin)
        .output()
        .map_err(|error| format!("inspect packaged plugin runtime dependencies: {error}"))?;
    if !dependencies.status.success() {
        return Err(format!(
            "packaged plugin otool inspection exited with {}",
            dependencies.status
        ));
    }
    let mut libraries = String::from_utf8_lossy(&dependencies.stdout)
        .lines()
        .skip(2)
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    libraries.sort();
    libraries.dedup();
    if libraries.is_empty() {
        return Err(
            "packaged plugin otool inspection found no shared runtime libraries".to_owned(),
        );
    }
    Ok(format!("macOS {version}; imports={}", libraries.join(",")))
}

fn windows_runtime_abi(plugin: &Path) -> Result<String, String> {
    let mut commands = vec![(PathBuf::from("dumpbin.exe"), vec!["/dependents"])];
    if let Some(cl) = visual_studio_cl() {
        commands.push((cl.with_file_name("dumpbin.exe"), vec!["/dependents"]));
    }
    commands.push((PathBuf::from("llvm-objdump.exe"), vec!["-p"]));
    let mut failures = Vec::new();
    for (program, arguments) in commands {
        let output = match Command::new(&program).args(arguments).arg(plugin).output() {
            Ok(output) => output,
            Err(error) => {
                failures.push(error.to_string());
                continue;
            }
        };
        if !output.status.success() {
            failures.push(output.status.to_string());
            continue;
        }
        let mut libraries = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(windows_dll_name)
            .collect::<Vec<_>>();
        libraries.sort();
        libraries.dedup();
        if !libraries.is_empty() {
            return Ok(format!("msvc-x64; imports={}", libraries.join(",")));
        }
        failures.push("inspector returned no DLL imports".to_owned());
    }
    Err(format!(
        "inspect packaged plugin MSVC runtime imports: {}",
        failures.join("; ")
    ))
}

fn windows_dll_name(line: &str) -> Option<String> {
    let value = line
        .trim()
        .strip_prefix("DLL Name:")
        .map(str::trim)
        .unwrap_or_else(|| line.trim());
    let token = value.split_whitespace().next()?;
    let normalized = token.to_ascii_lowercase();
    normalized.ends_with(".dll").then_some(normalized)
}

#[cfg(target_os = "windows")]
fn visual_studio_cl() -> Option<PathBuf> {
    let vswhere = PathBuf::from(env::var_os("ProgramFiles(x86)")?)
        .join("Microsoft Visual Studio/Installer/vswhere.exe");
    let output = Command::new(vswhere)
        .args([
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let root = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let mut versions = std::fs::read_dir(root.join("VC/Tools/MSVC"))
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    versions.sort_by(|left, right| right.cmp(left));
    versions
        .into_iter()
        .map(|version| version.join("bin/Hostx64/x64/cl.exe"))
        .find(|path| path.is_file())
}

#[cfg(not(target_os = "windows"))]
fn visual_studio_cl() -> Option<PathBuf> {
    None
}

fn one_line(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::one_line;

    #[test]
    fn evidence_values_are_single_line() {
        assert_eq!(
            one_line("rustc 1.2\nsecret\tvalue"),
            "rustc 1.2 secret value"
        );
    }
}
