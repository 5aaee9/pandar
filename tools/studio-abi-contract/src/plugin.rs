use std::{collections::BTreeSet, path::Path, process::Command};

#[cfg(target_os = "windows")]
use std::{io, path::PathBuf};

use crate::source::StudioContract;

pub struct ExportReport {
    pub count: usize,
    pub missing: Vec<String>,
}

pub fn verify_exports(plugin: &Path, contract: &StudioContract) -> Result<ExportReport, String> {
    let expected = contract
        .network_symbols
        .union(&contract.file_transfer_symbols)
        .cloned()
        .collect::<BTreeSet<_>>();
    verify_required_exports(plugin, &expected)
}

pub fn verify_required_exports(
    plugin: &Path,
    expected: &BTreeSet<String>,
) -> Result<ExportReport, String> {
    if !plugin.is_file() {
        return Err(format!(
            "plugin artifact is not a file: {}",
            plugin.display()
        ));
    }
    let exported = exported_symbols(plugin)?;
    let missing = expected.difference(&exported).cloned().collect::<Vec<_>>();
    Ok(ExportReport {
        count: exported.len(),
        missing,
    })
}

#[cfg(target_os = "windows")]
fn exported_symbols(plugin: &Path) -> Result<BTreeSet<String>, String> {
    let output = match Command::new("dumpbin").arg("/exports").arg(plugin).output() {
        Ok(output) => output,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let dumpbin = visual_studio_dumpbin().ok_or_else(|| {
                "dumpbin.exe is required; install Visual Studio C++ Build Tools".to_owned()
            })?;
            Command::new(&dumpbin)
                .arg("/exports")
                .arg(plugin)
                .output()
                .map_err(|error| format!("run {} /exports: {error}", dumpbin.display()))?
        }
        Err(error) => return Err(format!("run dumpbin /exports: {error}")),
    };
    parse_inspector_output(plugin, "dumpbin /exports", output)
}

#[cfg(not(target_os = "windows"))]
fn exported_symbols(plugin: &Path) -> Result<BTreeSet<String>, String> {
    let mut command = Command::new("nm");
    if cfg!(target_os = "macos") {
        command.args(["-gU"]);
    } else {
        command.args(["-gD", "--defined-only"]);
    }
    let output = command
        .arg(plugin)
        .output()
        .map_err(|error| format!("run native nm for {}: {error}", plugin.display()))?;
    parse_inspector_output(plugin, "native nm defined exports", output)
}

fn parse_inspector_output(
    plugin: &Path,
    inspector: &str,
    output: std::process::Output,
) -> Result<BTreeSet<String>, String> {
    if !output.status.success() {
        return Err(format!(
            "{inspector} failed for {} with {}: {}",
            plugin.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| inspected_symbol(line, cfg!(target_os = "windows")))
        .map(|symbol| symbol.trim_start_matches('_'))
        .filter(|symbol| symbol.starts_with("bambu_network_") || symbol.starts_with("ft_"))
        .map(str::to_owned)
        .collect())
}

fn inspected_symbol(line: &str, pe_dumpbin: bool) -> Option<&str> {
    let declaration = if pe_dumpbin {
        line.split_once('=').map_or(line, |(left, _)| left)
    } else {
        line
    };
    declaration.split_whitespace().last()
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
    let installation = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let mut versions = std::fs::read_dir(installation.join("VC/Tools/MSVC"))
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    versions.sort_by(|left, right| right.cmp(left));
    let target = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "x86" => "x86",
        "aarch64" => "arm64",
        arch => arch,
    };
    versions
        .into_iter()
        .map(|version| version.join("bin/Hostx64").join(target).join("dumpbin.exe"))
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        fs,
        path::{Path, PathBuf},
        process::Command,
    };

    use crate::source::StudioContract;

    use super::{inspected_symbol, verify_exports};

    fn dynamic_library_name() -> &'static str {
        if cfg!(windows) {
            "contract_plugin.dll"
        } else if cfg!(target_os = "macos") {
            "libcontract_plugin.dylib"
        } else {
            "libcontract_plugin.so"
        }
    }

    fn compile_plugin(root: &Path, exports: &[&str]) -> PathBuf {
        let source = root.join("plugin.rs");
        let contents = exports
            .iter()
            .map(|name| format!("#[unsafe(no_mangle)] pub extern \"C\" fn {name}() {{}}\n"))
            .collect::<String>();
        fs::write(&source, contents).expect("write test plugin source");
        let plugin = root.join(dynamic_library_name());
        let output = Command::new("rustc")
            .args(["--edition=2024", "--crate-type=cdylib"])
            .arg(source)
            .arg("-o")
            .arg(&plugin)
            .output()
            .expect("compile test plugin");
        assert!(
            output.status.success(),
            "compile test plugin failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        plugin
    }

    #[cfg(target_os = "linux")]
    fn compile_plugin_with_undefined_import(root: &Path) -> PathBuf {
        let source = root.join("undefined.c");
        fs::write(
            &source,
            "extern void ft_undefined_import(void);\n__attribute__((visibility(\"default\"))) void bambu_network_get_version(void) { ft_undefined_import(); }\n",
        )
        .expect("write undefined import plugin source");
        let plugin = root.join(dynamic_library_name());
        let output = Command::new("cc")
            .args(["-shared", "-fPIC"])
            .arg(source)
            .arg("-o")
            .arg(&plugin)
            .output()
            .expect("compile undefined import plugin");
        assert!(
            output.status.success(),
            "compile undefined import plugin failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        plugin
    }

    fn contract(network: &[&str], file_transfer: &[&str]) -> StudioContract {
        StudioContract {
            commit: "fixture".to_owned(),
            studio_version: "02.07.01.62".to_owned(),
            reference_network_agent_version: "02.07.01.51".to_owned(),
            network_symbols: network.iter().map(|value| (*value).to_owned()).collect(),
            file_transfer_symbols: file_transfer
                .iter()
                .map(|value| (*value).to_owned())
                .collect::<BTreeSet<_>>(),
            network_exports: Vec::new(),
            file_transfer_exports: Vec::new(),
            print_params_fields: Vec::new(),
        }
    }

    #[test]
    fn dumpbin_alias_keeps_the_exported_name() {
        assert_eq!(
            inspected_symbol(
                "109 6C 00000000 bambu_network_build_login_info = bambu_network_build_login_cmd",
                true,
            ),
            Some("bambu_network_build_login_info")
        );
    }

    #[test]
    fn accepts_library_exporting_every_loaded_upstream_symbol() {
        let temp = tempfile::tempdir().expect("create plugin fixture");
        let plugin = compile_plugin(
            temp.path(),
            &["bambu_network_get_version", "ft_abi_version"],
        );
        let contract = contract(&["bambu_network_get_version"], &["ft_abi_version"]);

        let report = verify_exports(&plugin, &contract).unwrap();
        assert_eq!(report.count, 2);
        assert!(report.missing.is_empty());
    }

    #[test]
    fn rejects_library_missing_a_loaded_upstream_symbol() {
        let temp = tempfile::tempdir().expect("create plugin fixture");
        let plugin = compile_plugin(temp.path(), &["bambu_network_get_version"]);
        let contract = contract(
            &["bambu_network_get_version", "bambu_network_start"],
            &["ft_abi_version"],
        );

        let report = verify_exports(&plugin, &contract).unwrap();

        assert_eq!(report.missing, ["bambu_network_start", "ft_abi_version"]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn does_not_count_same_prefix_undefined_import_as_an_export() {
        let temp = tempfile::tempdir().expect("create undefined import fixture");
        let plugin = compile_plugin_with_undefined_import(temp.path());
        let contract = contract(&["bambu_network_get_version"], &["ft_undefined_import"]);

        let report = verify_exports(&plugin, &contract).unwrap();

        assert_eq!(report.missing, ["ft_undefined_import"]);
    }
}
