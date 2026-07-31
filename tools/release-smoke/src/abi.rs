use std::{collections::BTreeSet, fs, path::Path, process::Command};

#[cfg(target_os = "windows")]
use std::{io, path::PathBuf};

use crate::host::NativeTarget;

const EXPORT_MAP_PATH: &str = "crates/pandar-network-plugin/src/shim_exports.hpp";
const NETWORK_COUNT: usize = 109;
const FILE_TRANSFER_COUNT: usize = 21;
const TOTAL_COUNT: usize = NETWORK_COUNT + FILE_TRANSFER_COUNT;
pub(crate) const SOURCE_SENTINEL: &str = "pandar_bambu_source_sentinel";

pub(crate) struct AbiSymbols {
    pub all: BTreeSet<String>,
    pub network_count: usize,
    pub file_transfer_count: usize,
}

pub(crate) struct ExportInspection {
    pub symbols: BTreeSet<String>,
    pub inspector: &'static str,
}

#[derive(Clone, Copy)]
enum SymbolOutput {
    Nm,
    Readelf,
    #[cfg(any(target_os = "windows", test))]
    PeDumpbin,
}

pub(crate) fn expected_symbols(repo_root: &Path) -> Result<AbiSymbols, String> {
    let path = repo_root.join(EXPORT_MAP_PATH);
    let content = fs::read_to_string(&path).map_err(|error| {
        format!(
            "read canonical Studio export map {}: {error}",
            path.display()
        )
    })?;
    parse_expected_symbols(&content)
}

fn parse_expected_symbols(content: &str) -> Result<AbiSymbols, String> {
    let mut all = BTreeSet::new();
    let mut network_count = 0;
    let mut file_transfer_count = 0;

    for (index, line) in content.lines().enumerate() {
        let Some(record) = line.trim().strip_prefix("PANDAR_STUDIO_EXPORT(") else {
            continue;
        };
        let symbol = record
            .split_once(',')
            .map(|(symbol, _)| symbol.trim())
            .ok_or_else(|| format!("invalid Studio export record at line {}", index + 1))?;
        match symbol {
            symbol if symbol.starts_with("bambu_network_") => network_count += 1,
            symbol if symbol.starts_with("ft_") => file_transfer_count += 1,
            _ => {
                return Err(format!(
                    "invalid Studio export symbol {symbol} at line {}",
                    index + 1
                ));
            }
        }
        if !all.insert(symbol.to_owned()) {
            return Err(format!("duplicate Studio export symbol {symbol}"));
        }
    }

    if network_count != NETWORK_COUNT
        || file_transfer_count != FILE_TRANSFER_COUNT
        || all.len() != TOTAL_COUNT
    {
        return Err(format!(
            "canonical Studio export map must contain exactly {NETWORK_COUNT} network and {FILE_TRANSFER_COUNT} FT symbols, got {network_count} network, {file_transfer_count} FT, {} total",
            all.len()
        ));
    }
    if !all.contains("bambu_network_sync_ams_filaments") {
        return Err(
            "canonical Studio export map is missing bambu_network_sync_ams_filaments".to_owned(),
        );
    }

    Ok(AbiSymbols {
        all,
        network_count,
        file_transfer_count,
    })
}

pub(crate) fn validate_exact_exports(
    expected: &BTreeSet<String>,
    actual: &BTreeSet<String>,
) -> Result<(), String> {
    let missing = expected.difference(actual).cloned().collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!("missing plugin exports: {}", missing.join(", ")));
    }
    let extra = actual.difference(expected).cloned().collect::<Vec<_>>();
    if !extra.is_empty() {
        return Err(format!(
            "unexpected target-prefix plugin exports: {}",
            extra.join(", ")
        ));
    }
    Ok(())
}

pub(crate) fn validate_source_exports(actual: &BTreeSet<String>) -> Result<(), String> {
    if !actual.contains(SOURCE_SENTINEL) {
        return Err(format!(
            "BambuSource companion is missing sentinel export {SOURCE_SENTINEL}"
        ));
    }
    let bambu = actual
        .iter()
        .filter(|symbol| symbol.starts_with("Bambu_"))
        .cloned()
        .collect::<Vec<_>>();
    if !bambu.is_empty() {
        return Err(format!(
            "BambuSource companion must not export camera/media entrypoints: {}",
            bambu.join(", ")
        ));
    }
    Ok(())
}

pub(crate) fn inspect_exports(
    target: NativeTarget,
    plugin: &Path,
) -> Result<ExportInspection, String> {
    if !plugin.is_file() {
        return Err("staged plugin artifact is not a file".to_owned());
    }
    match target {
        NativeTarget::LinuxAmd64 => inspect_with_candidates(
            plugin,
            &[
                ("nm", &["-D", "--defined-only"], SymbolOutput::Nm),
                ("readelf", &["-Ws"], SymbolOutput::Readelf),
            ],
            parse_exported_symbols,
        ),
        NativeTarget::MacosAmd64 | NativeTarget::MacosArm64 => inspect_with_candidates(
            plugin,
            &[("nm", &["-gU"], SymbolOutput::Nm)],
            parse_exported_symbols,
        ),
        NativeTarget::WindowsAmd64 => inspect_windows_exports(plugin, parse_exported_symbols),
    }
}

pub(crate) fn inspect_source_exports(
    target: NativeTarget,
    source: &Path,
) -> Result<ExportInspection, String> {
    if !source.is_file() {
        return Err("staged BambuSource companion is not a file".to_owned());
    }
    match target {
        NativeTarget::LinuxAmd64 => inspect_with_candidates(
            source,
            &[
                ("nm", &["-D", "--defined-only"], SymbolOutput::Nm),
                ("readelf", &["-Ws"], SymbolOutput::Readelf),
            ],
            parse_source_exports,
        ),
        NativeTarget::MacosAmd64 | NativeTarget::MacosArm64 => inspect_with_candidates(
            source,
            &[("nm", &["-gU"], SymbolOutput::Nm)],
            parse_source_exports,
        ),
        NativeTarget::WindowsAmd64 => inspect_windows_exports(source, parse_source_exports),
    }
}

fn inspect_with_candidates(
    plugin: &Path,
    candidates: &[(&'static str, &'static [&'static str], SymbolOutput)],
    parser: fn(SymbolOutput, &str) -> BTreeSet<String>,
) -> Result<ExportInspection, String> {
    let mut failures = Vec::new();
    for (program, args, kind) in candidates {
        match Command::new(program).args(*args).arg(plugin).output() {
            Ok(output) if output.status.success() => {
                return Ok(ExportInspection {
                    symbols: parser(*kind, &String::from_utf8_lossy(&output.stdout)),
                    inspector: program,
                });
            }
            Ok(output) => failures.push(format!("{program} exited with {}", output.status)),
            Err(error) => failures.push(format!("{program}: {error}")),
        }
    }
    Err(format!(
        "no native plugin export inspector succeeded: {}",
        failures.join("; ")
    ))
}

#[cfg(not(target_os = "windows"))]
fn inspect_windows_exports(
    _plugin: &Path,
    _parser: fn(SymbolOutput, &str) -> BTreeSet<String>,
) -> Result<ExportInspection, String> {
    Err("Windows plugin inspection requires a native Windows host".to_owned())
}

#[cfg(target_os = "windows")]
fn inspect_windows_exports(
    plugin: &Path,
    parser: fn(SymbolOutput, &str) -> BTreeSet<String>,
) -> Result<ExportInspection, String> {
    let dumpbin = Command::new("dumpbin").arg("/exports").arg(plugin).output();
    let dumpbin = match dumpbin {
        Ok(output) => Some(output),
        Err(error) if error.kind() == io::ErrorKind::NotFound => visual_studio_dumpbin()
            .map(|path| Command::new(path).arg("/exports").arg(plugin).output())
            .transpose()
            .map_err(|error| format!("run native dumpbin /exports: {error}"))?,
        Err(error) => return Err(format!("run native dumpbin /exports: {error}")),
    };
    if let Some(output) = dumpbin
        && output.status.success()
    {
        return Ok(ExportInspection {
            symbols: parser(
                SymbolOutput::PeDumpbin,
                &String::from_utf8_lossy(&output.stdout),
            ),
            inspector: "dumpbin",
        });
    }
    inspect_with_candidates(
        plugin,
        &[("llvm-nm", &["-g", "--defined-only"], SymbolOutput::Nm)],
        parser,
    )
}

#[cfg(target_os = "windows")]
fn visual_studio_dumpbin() -> Option<PathBuf> {
    let vswhere = PathBuf::from(std::env::var_os("ProgramFiles(x86)")?)
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
    let mut versions = fs::read_dir(root.join("VC/Tools/MSVC"))
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    versions.sort_by(|left, right| right.cmp(left));
    versions
        .into_iter()
        .map(|version| version.join("bin/Hostx64/x64/dumpbin.exe"))
        .find(|path| path.is_file())
}

fn parse_exported_symbols(kind: SymbolOutput, output: &str) -> BTreeSet<String> {
    parse_matching_exports(kind, output, exported_symbol_token)
}

fn parse_source_exports(kind: SymbolOutput, output: &str) -> BTreeSet<String> {
    parse_matching_exports(kind, output, source_symbol_token)
}

fn parse_matching_exports(
    kind: SymbolOutput,
    output: &str,
    select: fn(&str) -> Option<String>,
) -> BTreeSet<String> {
    output
        .lines()
        .filter_map(|line| match kind {
            SymbolOutput::Nm => {
                let fields = line.split_whitespace().collect::<Vec<_>>();
                if fields.len() < 2 || fields[fields.len() - 2].eq_ignore_ascii_case("u") {
                    None
                } else {
                    fields.last().and_then(|token| select(token))
                }
            }
            SymbolOutput::Readelf => {
                let fields = line.split_whitespace().collect::<Vec<_>>();
                if fields.len() < 8 || fields.contains(&"UND") {
                    None
                } else {
                    fields.last().and_then(|token| select(token))
                }
            }
            #[cfg(any(target_os = "windows", test))]
            SymbolOutput::PeDumpbin => line
                .split_once('=')
                .map_or(line, |(left, _)| left)
                .split_whitespace()
                .last()
                .and_then(select),
        })
        .collect()
}

fn exported_symbol_token(token: &str) -> Option<String> {
    let symbol = normalized_symbol(token);
    (symbol.starts_with("bambu_network_") || symbol.starts_with("ft_")).then(|| symbol.to_owned())
}

fn source_symbol_token(token: &str) -> Option<String> {
    let symbol = normalized_symbol(token);
    (symbol == SOURCE_SENTINEL || symbol.starts_with("Bambu_")).then(|| symbol.to_owned())
}

fn normalized_symbol(token: &str) -> &str {
    let normalized = token.strip_prefix('_').unwrap_or(token);
    normalized
        .split_once('@')
        .map_or(normalized, |(symbol, _)| symbol)
}

#[cfg(test)]
mod tests;
