use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

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

fn exported_symbols(path: &Path) -> BTreeSet<String> {
    if cfg!(target_os = "windows") {
        let output = Command::new("dumpbin")
            .arg("/exports")
            .arg(path)
            .output()
            .expect("dumpbin /exports is required to inspect Windows plugin exports");
        assert!(
            output.status.success(),
            "dumpbin /exports failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return String::from_utf8_lossy(&output.stdout)
            .lines()
            .flat_map(|line| line.split_whitespace().last())
            .filter(|symbol| symbol.starts_with("bambu_network_") || symbol.starts_with("ft_"))
            .map(ToOwned::to_owned)
            .collect();
    }

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
