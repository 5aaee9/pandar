use std::collections::BTreeSet;
#[cfg(target_os = "windows")]
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(target_os = "windows")]
use std::process::Output;
fn target_dir() -> PathBuf {
    if let Some(configured) = std::env::var_os("CARGO_TARGET_DIR")
        && !configured.is_empty()
    {
        let configured = PathBuf::from(configured);
        return if configured.is_absolute() {
            configured
        } else {
            std::env::current_dir()
                .expect("resolve current directory for CARGO_TARGET_DIR")
                .join(configured)
        };
    }

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

fn target_studio_symbols() -> BTreeSet<String> {
    let abi_series = selected_abi_series();
    let symbols = include_str!("../src/shim_exports.hpp")
        .lines()
        .filter_map(|line| line.trim().strip_prefix("PANDAR_STUDIO_EXPORT("))
        .map(|record| {
            record
                .split_once(',')
                .map(|(symbol, _)| symbol.trim())
                .filter(|symbol| symbol.starts_with("bambu_network_") || symbol.starts_with("ft_"))
                .unwrap_or_else(|| panic!("invalid target Studio export record: {record}"))
                .to_owned()
        })
        .filter(|symbol| {
            abi_series.capabilities.filament_cloud || !is_filament_cloud_symbol(symbol)
        })
        .filter(|symbol| {
            abi_series.capabilities.ams_sync || symbol != "bambu_network_sync_ams_filaments"
        })
        .filter(|symbol| {
            abi_series.capabilities.slot_mappings_sync
                || symbol != "bambu_network_sync_slot_mappings"
        })
        .collect::<Vec<_>>();
    let expected = symbols.iter().cloned().collect::<BTreeSet<_>>();
    assert_eq!(
        expected.len(),
        symbols.len(),
        "duplicate target Studio export"
    );
    assert_eq!(
        symbols
            .iter()
            .filter(|symbol| symbol.starts_with("bambu_network_"))
            .count(),
        abi_series.network_exports
    );
    assert_eq!(
        symbols
            .iter()
            .filter(|symbol| symbol.starts_with("ft_"))
            .count(),
        abi_series.file_transfer_exports
    );
    expected
}

fn selected_abi_series() -> &'static pandar_studio_profile::StudioAbiSeries {
    pandar_studio_profile::abi_series(pandar_network_plugin::STUDIO_ABI_SERIES).unwrap()
}

fn is_filament_cloud_symbol(symbol: &str) -> bool {
    matches!(
        symbol,
        "bambu_network_get_filament_spools"
            | "bambu_network_create_filament_spool"
            | "bambu_network_update_filament_spool"
            | "bambu_network_delete_filament_spools"
            | "bambu_network_get_filament_config"
    )
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
        let mut command = Command::new("nm");
        if cfg!(target_os = "macos") {
            command.arg("-gU");
        } else {
            command.args(["-gD", "--defined-only"]);
        }
        let output = command
            .arg(path)
            .output()
            .expect("native nm is required to inspect defined plugin exports");
        assert!(
            output.status.success(),
            "native nm defined-export inspection failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.split_whitespace().last())
            .map(|symbol| {
                if cfg!(target_os = "macos") {
                    symbol.strip_prefix('_').unwrap_or(symbol)
                } else {
                    symbol
                }
            })
            .filter(|symbol| symbol.starts_with("bambu_network_") || symbol.starts_with("ft_"))
            .map(ToOwned::to_owned)
            .collect()
    }
}

#[test]
fn exports_exact_target_studio_abi() {
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
        "dynamic library does not exist at {}",
        library.display()
    );

    assert_eq!(exported_symbols(&library), target_studio_symbols());
}
