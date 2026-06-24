use std::collections::BTreeSet;
#[cfg(target_os = "windows")]
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(target_os = "windows")]
use std::process::Output;

fn target_dir() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop();
    dir.pop();
    dir.join("target")
}

fn dynamic_library_path() -> PathBuf {
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let filename = if cfg!(target_os = "windows") {
        "pandar_network_plugin.dll"
    } else if cfg!(target_os = "macos") {
        "libpandar_network_plugin.dylib"
    } else {
        "libpandar_network_plugin.so"
    };
    target_dir().join(profile).join(filename)
}

fn expected_symbols() -> BTreeSet<String> {
    let symbols = include_str!(
        "../../../docs/superpowers/specs/2026-06-23-phase-21-network-plugin-abi-symbols.txt"
    );
    symbols
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("bambu_network_") || line.starts_with("ft_"))
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(target_os = "windows")]
fn visual_studio_dumpbin() -> Option<PathBuf> {
    let vswhere = PathBuf::from(std::env::var_os("ProgramFiles(x86)")?)
        .join("Microsoft Visual Studio")
        .join("Installer")
        .join("vswhere.exe");
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

    let installation_path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if installation_path.is_empty() {
        return None;
    }

    let installation = PathBuf::from(installation_path);
    let msvc_root = installation.join("VC").join("Tools").join("MSVC");
    let mut versions = std::fs::read_dir(msvc_root)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().map(|ty| ty.is_dir()).unwrap_or(false))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    versions.sort();
    versions.reverse();

    let target = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "x86" => "x86",
        "aarch64" => "arm64",
        arch => arch,
    };

    for version in &versions {
        let dumpbin = version
            .join("bin")
            .join("Hostx64")
            .join(target)
            .join("dumpbin.exe");
        if dumpbin.exists() {
            return Some(dumpbin);
        }
    }

    for version in versions {
        let bin = version.join("bin");
        let mut hosts = std::fs::read_dir(bin)
            .ok()?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().map(|ty| ty.is_dir()).unwrap_or(false))
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        hosts.sort();

        for host in hosts {
            let dumpbin = host.join(target).join("dumpbin.exe");
            if dumpbin.exists() {
                return Some(dumpbin);
            }

            let mut targets = std::fs::read_dir(host)
                .ok()?
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().map(|ty| ty.is_dir()).unwrap_or(false))
                .map(|entry| entry.path())
                .collect::<Vec<_>>();
            targets.sort();
            for target_dir in targets {
                let dumpbin = target_dir.join("dumpbin.exe");
                if dumpbin.exists() {
                    return Some(dumpbin);
                }
            }
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn dumpbin_exports(path: &Path) -> Output {
    match Command::new("dumpbin").arg("/exports").arg(path).output() {
        Ok(output) => output,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            let dumpbin = visual_studio_dumpbin().unwrap_or_else(|| {
                panic!(
                    "dumpbin /exports is required to inspect Windows plugin exports; \
                     add dumpbin.exe to PATH or install Visual Studio C++ Build Tools"
                )
            });
            Command::new(&dumpbin)
                .arg("/exports")
                .arg(path)
                .output()
                .unwrap_or_else(|err| panic!("failed to run {} /exports: {err}", dumpbin.display()))
        }
        Err(err) => panic!("failed to run dumpbin /exports: {err}"),
    }
}

fn exported_symbols(path: &Path) -> BTreeSet<String> {
    #[cfg(target_os = "windows")]
    {
        let output = dumpbin_exports(path);
        assert!(
            output.status.success(),
            "dumpbin /exports failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .flat_map(|line| line.split_whitespace().last())
            .filter(|symbol| symbol.starts_with("bambu_network_") || symbol.starts_with("ft_"))
            .map(ToOwned::to_owned)
            .collect()
    }

    #[cfg(not(target_os = "windows"))]
    {
        let output = Command::new("nm")
            .arg("-g")
            .arg(path)
            .output()
            .expect("nm -g is required to inspect plugin exports");
        assert!(
            output.status.success(),
            "nm -g failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.split_whitespace().last())
            .filter(|symbol| symbol.starts_with("bambu_network_") || symbol.starts_with("ft_"))
            .map(ToOwned::to_owned)
            .collect()
    }
}

#[test]
fn exports_phase_21_abi_symbols() {
    let library = dynamic_library_path();
    let status = Command::new("cargo")
        .args(["build", "-p", "pandar-network-plugin"])
        .status()
        .expect("cargo build -p pandar-network-plugin is required before export inspection");
    assert!(
        status.success(),
        "cargo build -p pandar-network-plugin failed"
    );
    assert!(
        library.exists(),
        "dynamic library does not exist at {}; run cargo build -p pandar-network-plugin first",
        library.display()
    );

    let expected = expected_symbols();
    let exported = exported_symbols(&library);
    let missing = expected.difference(&exported).cloned().collect::<Vec<_>>();

    assert!(missing.is_empty(), "missing plugin exports: {missing:?}");
}
