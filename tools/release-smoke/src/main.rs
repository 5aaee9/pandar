use std::{
    collections::BTreeSet,
    env, fs,
    fs::File,
    io::Read,
    path::{Component, Path, PathBuf},
    process::{Command, ExitCode},
};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use tar::Archive;

struct Args {
    label: String,
    runner_os: String,
    archive: PathBuf,
    checksum: PathBuf,
    cli_name: String,
    plugin_name: String,
    repo_root: PathBuf,
}

struct ChecksumSidecar {
    digest: String,
    archive_name: String,
}

#[derive(Debug, Eq, PartialEq)]
enum CliStartup {
    Run,
    Skip(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PluginInspection {
    Elf,
    MachO,
    Pe,
}

#[derive(Clone, Copy)]
enum SymbolOutput {
    Nm,
    Readelf,
    PeObjdump,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("release smoke failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let (cli_startup, plugin_inspection) = support_for(&args.label, &args.runner_os)?;
    validate_checksum_file(&args)?;
    let stage = unpack_archive(&args)?;
    validate_layout(&args, &stage)?;
    println!("PASS archive layout: {}", args.label);
    validate_cli_startup(&args, &stage, cli_startup)?;
    let expected = expected_symbols(&args.repo_root)?;
    let actual = exported_symbols(plugin_inspection, &stage.join(&args.plugin_name))?;
    validate_plugin_exports(&expected, &actual)?;
    Ok(())
}

fn parse_args() -> Result<Args, String> {
    let mut label = None;
    let mut runner_os = None;
    let mut archive = None;
    let mut checksum = None;
    let mut cli_name = None;
    let mut plugin_name = None;
    let mut repo_root = None;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args.next().ok_or_else(usage)?;
        match arg.as_str() {
            "--label" => label = Some(value),
            "--runner-os" => runner_os = Some(value),
            "--archive" => archive = Some(PathBuf::from(value)),
            "--checksum" => checksum = Some(PathBuf::from(value)),
            "--cli-name" => cli_name = Some(value),
            "--plugin-name" => plugin_name = Some(value),
            "--repo-root" => repo_root = Some(PathBuf::from(value)),
            _ => return Err(usage()),
        }
    }

    Ok(Args {
        label: label.ok_or_else(usage)?,
        runner_os: runner_os.ok_or_else(usage)?,
        archive: archive.ok_or_else(usage)?,
        checksum: checksum.ok_or_else(usage)?,
        cli_name: cli_name.ok_or_else(usage)?,
        plugin_name: plugin_name.ok_or_else(usage)?,
        repo_root: repo_root.ok_or_else(usage)?,
    })
}

fn usage() -> String {
    "usage: pandar-release-smoke --label <target-label> --runner-os <linux|macos|windows> --archive <path> --checksum <path> --cli-name <filename> --plugin-name <filename> --repo-root <path>".to_owned()
}

fn validate_checksum_file(args: &Args) -> Result<(), String> {
    let sidecar =
        parse_checksum_sidecar(&fs::read_to_string(&args.checksum).map_err(|error| {
            format!(
                "failed to read checksum sidecar {}: {error}",
                args.checksum.display()
            )
        })?)?;

    let archive_name = args
        .archive
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("archive path has no file name: {}", args.archive.display()))?;
    if sidecar.archive_name != archive_name {
        return Err(format!(
            "checksum sidecar names {}, expected {archive_name}",
            sidecar.archive_name
        ));
    }

    let actual = sha256_hex(&args.archive)?;
    if actual != sidecar.digest.to_ascii_lowercase() {
        return Err(format!(
            "checksum mismatch for {}: expected {}, got {actual}",
            args.archive.display(),
            sidecar.digest
        ));
    }

    Ok(())
}

fn parse_checksum_sidecar(content: &str) -> Result<ChecksumSidecar, String> {
    let mut lines = content.lines().filter(|line| !line.trim().is_empty());
    let line = lines
        .next()
        .ok_or_else(|| "checksum sidecar must contain exactly one non-empty line".to_owned())?;
    if lines.next().is_some() {
        return Err("checksum sidecar must contain exactly one non-empty line".to_owned());
    }

    let mut fields = line.split_whitespace();
    let digest = fields
        .next()
        .ok_or_else(|| "checksum sidecar must contain digest and archive name".to_owned())?;
    let archive_name = fields
        .next()
        .ok_or_else(|| "checksum sidecar must contain digest and archive name".to_owned())?;
    if fields.next().is_some() {
        return Err("checksum sidecar must contain exactly two fields".to_owned());
    }

    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("checksum digest must be 64 hex characters".to_owned());
    }

    let archive_path = Path::new(archive_name);
    if archive_path.components().count() != 1 {
        return Err("checksum archive name must not be a path".to_owned());
    }

    Ok(ChecksumSidecar {
        digest: digest.to_owned(),
        archive_name: archive_name.to_owned(),
    })
}

fn sha256_hex(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("failed to open archive {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];

    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("failed to read archive {}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn unpack_archive(args: &Args) -> Result<PathBuf, String> {
    let stage = env::temp_dir().join(format!(
        "pandar-release-smoke-{}-{}",
        std::process::id(),
        args.label
    ));
    if stage.exists() {
        fs::remove_dir_all(&stage).map_err(|error| {
            format!(
                "failed to remove existing stage directory {}: {error}",
                stage.display()
            )
        })?;
    }
    fs::create_dir_all(&stage).map_err(|error| {
        format!(
            "failed to create stage directory {}: {error}",
            stage.display()
        )
    })?;

    let archive_file = File::open(&args.archive)
        .map_err(|error| format!("failed to open archive {}: {error}", args.archive.display()))?;
    let decoder = GzDecoder::new(archive_file);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries().map_err(|error| {
        format!(
            "failed to read archive entries from {}: {error}",
            args.archive.display()
        )
    })? {
        let mut entry = entry.map_err(|error| format!("failed to read archive entry: {error}"))?;
        let normalized = normalized_top_level_path(&entry.path().map_err(|error| {
            format!(
                "failed to read archive entry path from {}: {error}",
                args.archive.display()
            )
        })?)?;

        let Some(relative) = normalized else {
            continue;
        };
        entry.unpack(stage.join(relative)).map_err(|error| {
            format!(
                "failed to unpack archive entry into {}: {error}",
                stage.display()
            )
        })?;
    }

    Ok(stage)
}

fn normalized_top_level_path(path: &Path) -> Result<Option<PathBuf>, String> {
    let mut components = path.components().peekable();
    while matches!(components.peek(), Some(Component::CurDir)) {
        components.next();
    }

    let mut normalized = Vec::new();
    for component in components {
        match component {
            Component::Normal(name) => normalized.push(name.to_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(format!(
                    "archive entry {} contains parent-directory traversal",
                    path.display()
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!("archive entry {} is absolute", path.display()));
            }
        }
    }

    match normalized.as_slice() {
        [] => Ok(None),
        [_] => Ok(Some(normalized.into_iter().collect())),
        _ => Err(format!(
            "archive entry {} must be a top-level file",
            path.display()
        )),
    }
}

fn validate_layout(args: &Args, stage: &Path) -> Result<(), String> {
    let actual = fs::read_dir(stage)
        .map_err(|error| {
            format!(
                "failed to read stage directory {}: {error}",
                stage.display()
            )
        })?
        .map(|entry| {
            let entry = entry.map_err(|error| {
                format!(
                    "failed to read stage directory entry {}: {error}",
                    stage.display()
                )
            })?;
            let file_type = entry.file_type().map_err(|error| {
                format!(
                    "failed to read staged file type {}: {error}",
                    entry.path().display()
                )
            })?;
            if !file_type.is_file() {
                return Err(format!(
                    "archive entry {} must unpack as a file",
                    entry.path().display()
                ));
            }
            entry.file_name().into_string().map_err(|_| {
                format!(
                    "archive entry {} is not valid UTF-8",
                    entry.path().display()
                )
            })
        })
        .collect::<Result<BTreeSet<_>, _>>()?;

    let expected = BTreeSet::from([args.cli_name.clone(), args.plugin_name.clone()]);
    if actual != expected {
        return Err(format!(
            "archive layout mismatch: expected {:?}, got {:?}",
            expected, actual
        ));
    }

    Ok(())
}

fn expected_symbols(repo_root: &Path) -> Result<BTreeSet<String>, String> {
    let path =
        repo_root.join("docs/superpowers/specs/2026-06-23-phase-21-network-plugin-abi-symbols.txt");
    let content = fs::read_to_string(&path).map_err(|error| {
        format!(
            "failed to read expected symbols {}: {error}",
            path.display()
        )
    })?;
    parse_expected_symbols(&content)
}

fn parse_expected_symbols(content: &str) -> Result<BTreeSet<String>, String> {
    let symbols = content
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("bambu_network_") || line.starts_with("ft_"))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();

    if symbols.is_empty() {
        return Err("expected symbols file did not contain ABI symbols".to_owned());
    }

    Ok(symbols)
}

fn support_for(label: &str, runner_os: &str) -> Result<(CliStartup, PluginInspection), String> {
    match (label, runner_os) {
        ("linux-amd64", "linux") => Ok((CliStartup::Run, PluginInspection::Elf)),
        ("linux-arm64", "linux") => Ok((CliStartup::Skip("cross arch"), PluginInspection::Elf)),
        ("windows-amd64", "linux") | ("windows-arm64", "linux") => Ok((
            CliStartup::Skip("PE cannot run on Ubuntu"),
            PluginInspection::Pe,
        )),
        ("macos-amd64", "macos") | ("macos-arm64", "macos") => {
            Ok((CliStartup::Run, PluginInspection::MachO))
        }
        ("linux-amd64" | "linux-arm64", _) => Err(format!(
            "release label {label} must be validated on linux, got {runner_os}"
        )),
        ("windows-amd64" | "windows-arm64", _) => Err(format!(
            "release label {label} must be validated on linux, got {runner_os}"
        )),
        ("macos-amd64" | "macos-arm64", _) => Err(format!(
            "release label {label} must be validated on macos, got {runner_os}"
        )),
        _ => Err(format!("unsupported release label {label}")),
    }
}

fn validate_cli_startup(args: &Args, stage: &Path, startup: CliStartup) -> Result<(), String> {
    match startup {
        CliStartup::Run => {
            let cli = stage.join(&args.cli_name);
            let output = Command::new(&cli).arg("--help").output().map_err(|error| {
                format!(
                    "failed to run CLI startup {} --help: {error}",
                    cli.display()
                )
            })?;
            if !output.status.success() {
                return Err(format!(
                    "CLI startup failed for {} --help: status {} stderr {}",
                    cli.display(),
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
            println!("PASS cli-startup: {}", args.label);
            Ok(())
        }
        CliStartup::Skip(reason) => {
            println!(
                "SKIP cli-startup: {reason} (label={} runner={})",
                args.label, args.runner_os
            );
            Ok(())
        }
    }
}

fn exported_symbols(kind: PluginInspection, plugin: &Path) -> Result<BTreeSet<String>, String> {
    let commands: &[(&str, &[&str], SymbolOutput)] = match kind {
        PluginInspection::Elf => &[
            ("nm", &["-D", "--defined-only"], SymbolOutput::Nm),
            ("readelf", &["-Ws"], SymbolOutput::Readelf),
        ],
        PluginInspection::MachO => &[("nm", &["-gU"], SymbolOutput::Nm)],
        PluginInspection::Pe => &[
            ("objdump", &["-p"], SymbolOutput::PeObjdump),
            ("llvm-objdump", &["-p"], SymbolOutput::PeObjdump),
            ("llvm-nm", &["-g", "--defined-only"], SymbolOutput::Nm),
        ],
    };

    let mut failures = Vec::new();
    for (program, args, output_kind) in commands {
        let output = Command::new(program).args(*args).arg(plugin).output();
        let output = match output {
            Ok(output) => output,
            Err(error) => {
                failures.push(format!("{program}: {error}"));
                continue;
            }
        };
        if !output.status.success() {
            failures.push(format!(
                "{program}: status {} stderr {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
            continue;
        }

        return Ok(parse_exported_symbols(
            *output_kind,
            &String::from_utf8_lossy(&output.stdout),
        ));
    }

    Err(format!(
        "failed to inspect plugin exports {}: {}",
        plugin.display(),
        failures.join("; ")
    ))
}

fn parse_exported_symbols(kind: SymbolOutput, output: &str) -> BTreeSet<String> {
    match kind {
        SymbolOutput::Nm => parse_nm_symbols(output),
        SymbolOutput::Readelf => parse_readelf_symbols(output),
        SymbolOutput::PeObjdump => parse_pe_objdump_symbols(output),
    }
}

fn parse_nm_symbols(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .filter_map(|token| {
            let fields = token.split_whitespace().collect::<Vec<_>>();
            if fields.len() < 2 || token.contains("(undefined)") {
                return None;
            }
            let symbol_type = fields[fields.len() - 2];
            if symbol_type == "U" || symbol_type == "u" {
                return None;
            }
            exported_symbol_token(fields[fields.len() - 1])
        })
        .collect()
}

fn parse_readelf_symbols(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() < 8 || fields.contains(&"UND") {
                return None;
            }
            exported_symbol_token(fields[fields.len() - 1])
        })
        .collect()
}

fn parse_pe_objdump_symbols(output: &str) -> BTreeSet<String> {
    let mut in_export_table = false;
    output
        .lines()
        .filter_map(|line| {
            if line.contains("Export Table") || line.contains("Export Tables") {
                in_export_table = true;
                return None;
            }
            if line.contains("Import Table") || line.contains("Import Tables") {
                in_export_table = false;
                return None;
            }
            if !in_export_table {
                return None;
            }
            exported_symbol_token(line.split_whitespace().last()?)
        })
        .collect()
}

fn exported_symbol_token(token: &str) -> Option<String> {
    let normalized = token.strip_prefix('_').unwrap_or(token);
    if normalized.starts_with("bambu_network_") || normalized.starts_with("ft_") {
        Some(
            normalized
                .split_once('@')
                .map_or(normalized, |(symbol, _)| symbol)
                .to_owned(),
        )
    } else {
        None
    }
}

fn validate_plugin_exports(
    expected: &BTreeSet<String>,
    actual: &BTreeSet<String>,
) -> Result<(), String> {
    let missing = expected.difference(actual).cloned().collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!("missing plugin exports: {:?}", missing));
    }

    println!("PASS plugin exports: {} symbols", expected.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use std::io::Cursor;
    use tar::{Builder, Header};
    use tempfile::tempdir;

    #[test]
    fn checksum_sidecar_parsing_rejects_missing_lines() {
        assert!(parse_checksum_sidecar("").is_err());
        assert!(parse_checksum_sidecar("\n\n").is_err());
    }

    #[test]
    fn checksum_sidecar_parsing_rejects_path_valued_archive_names() {
        let content = format!("{} {}\n", "a".repeat(64), "nested/archive.tar.gz");
        assert!(parse_checksum_sidecar(&content).is_err());
    }

    #[test]
    fn checksum_sidecar_parsing_rejects_non_hex_checksum_strings() {
        let content = format!("{} archive.tar.gz\n", "z".repeat(64));
        assert!(parse_checksum_sidecar(&content).is_err());
    }

    #[test]
    fn checksum_validation_rejects_digest_mismatch() {
        let temp = tempdir().unwrap();
        let archive = temp.path().join("archive.tar.gz");
        let checksum = temp.path().join("archive.tar.gz.sha256");
        fs::write(&archive, b"archive bytes").unwrap();
        fs::write(&checksum, format!("{} archive.tar.gz\n", "0".repeat(64))).unwrap();

        let args = test_args(archive, checksum);

        assert!(validate_checksum_file(&args).is_err());
    }

    #[test]
    fn tar_path_normalization_accepts_dot_slash_file() {
        assert_eq!(
            normalized_top_level_path(Path::new("./pandar")).unwrap(),
            Some(PathBuf::from("pandar"))
        );
    }

    #[test]
    fn tar_path_normalization_rejects_nested_file() {
        assert!(normalized_top_level_path(Path::new("nested/pandar")).is_err());
    }

    #[test]
    fn tar_path_normalization_rejects_parent_directory() {
        assert!(normalized_top_level_path(Path::new("../pandar")).is_err());
    }

    #[test]
    fn tar_path_normalization_ignores_dot_directory() {
        assert_eq!(normalized_top_level_path(Path::new("./")).unwrap(), None);
    }

    #[test]
    fn unpack_and_layout_accepts_exact_top_level_files() {
        let temp = tempdir().unwrap();
        let archive = temp.path().join("archive.tar.gz");
        create_tar_gz(
            &archive,
            &[
                ("./", None),
                ("./pandar", Some(b"cli".as_slice())),
                ("./libpandar_network_plugin.so", Some(b"plugin".as_slice())),
            ],
        );
        let checksum = temp.path().join("archive.tar.gz.sha256");
        fs::write(
            &checksum,
            format!("{} archive.tar.gz\n", sha256_hex(&archive).unwrap()),
        )
        .unwrap();
        let args = test_args(archive, checksum);

        validate_checksum_file(&args).unwrap();
        let stage = unpack_archive(&args).unwrap();
        validate_layout(&args, &stage).unwrap();
    }

    #[test]
    fn expected_symbols_reads_canonical_symbol_lines() {
        let temp = tempdir().unwrap();
        let path = temp
            .path()
            .join("docs/superpowers/specs/2026-06-23-phase-21-network-plugin-abi-symbols.txt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "\n# header\n  bambu_network_get_version  \nignored\nft_abi_version\n",
        )
        .unwrap();

        assert_eq!(
            expected_symbols(temp.path()).unwrap(),
            BTreeSet::from([
                "bambu_network_get_version".to_owned(),
                "ft_abi_version".to_owned()
            ])
        );
    }

    #[test]
    fn expected_symbols_rejects_empty_symbol_list() {
        assert!(parse_expected_symbols("# no symbols\nignored\n").is_err());
    }

    #[test]
    fn support_for_skips_windows_cli_and_uses_pe_on_linux() {
        assert_eq!(
            support_for("windows-amd64", "linux").unwrap(),
            (
                CliStartup::Skip("PE cannot run on Ubuntu"),
                PluginInspection::Pe
            )
        );
    }

    #[test]
    fn support_for_rejects_linux_amd64_on_macos() {
        assert!(support_for("linux-amd64", "macos").is_err());
    }

    #[test]
    fn exported_symbol_parser_strips_one_leading_underscore() {
        assert_eq!(
            parse_exported_symbols(
                SymbolOutput::Nm,
                "0000 T _bambu_network_get_version\n__ignored\n0000 T _ft_abi_version"
            ),
            BTreeSet::from([
                "bambu_network_get_version".to_owned(),
                "ft_abi_version".to_owned()
            ])
        );
    }

    #[test]
    fn exported_symbol_parser_rejects_undefined_nm_symbols() {
        assert_eq!(
            parse_exported_symbols(
                SymbolOutput::Nm,
                "                 U bambu_network_get_version\n0000 T ft_abi_version"
            ),
            BTreeSet::from(["ft_abi_version".to_owned()])
        );
    }

    #[test]
    fn exported_symbol_parser_rejects_undefined_readelf_symbols() {
        assert_eq!(
            parse_exported_symbols(
                SymbolOutput::Readelf,
                "1: 0000000000000000 0 FUNC GLOBAL DEFAULT UND bambu_network_get_version\n2: 0000000000001000 0 FUNC GLOBAL DEFAULT 12 ft_abi_version"
            ),
            BTreeSet::from(["ft_abi_version".to_owned()])
        );
    }

    #[test]
    fn exported_symbol_parser_rejects_pe_import_table_symbols() {
        assert_eq!(
            parse_exported_symbols(
                SymbolOutput::PeObjdump,
                "The Export Tables\n[ 0] bambu_network_get_version\nThe Import Tables\n[ 1] ft_abi_version"
            ),
            BTreeSet::from(["bambu_network_get_version".to_owned()])
        );
    }

    fn test_args(archive: PathBuf, checksum: PathBuf) -> Args {
        Args {
            label: "linux-amd64".to_owned(),
            runner_os: "linux".to_owned(),
            archive,
            checksum,
            cli_name: "pandar".to_owned(),
            plugin_name: "libpandar_network_plugin.so".to_owned(),
            repo_root: PathBuf::from("."),
        }
    }

    fn create_tar_gz(path: &Path, entries: &[(&str, Option<&[u8]>)]) {
        let file = File::create(path).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        for (name, content) in entries {
            match content {
                Some(content) => {
                    let mut header = Header::new_gnu();
                    header.set_size(content.len() as u64);
                    header.set_cksum();
                    builder
                        .append_data(&mut header, *name, Cursor::new(*content))
                        .unwrap();
                }
                None => {
                    let mut header = Header::new_gnu();
                    header.set_entry_type(tar::EntryType::Directory);
                    header.set_size(0);
                    header.set_cksum();
                    builder
                        .append_data(&mut header, *name, &mut &[][..])
                        .unwrap();
                }
            }
        }

        builder.finish().unwrap();
    }
}
