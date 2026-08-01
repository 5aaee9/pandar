mod abi;
mod archive;
mod args;
mod evidence;
mod host;
mod process;

use std::time::Duration;

use abi::{
    SOURCE_SENTINEL, expected_symbols, inspect_exports, inspect_source_exports,
    validate_exact_exports, validate_source_exports,
};
use archive::{sha256_hex, stage_archive, validate_checksum};
use args::parse_args;
use evidence::{EvidenceInput, collect_evidence};
use host::validate_current_host;
use process::{run_abi_probe, run_packaged_cli};

const CLI_TIMEOUT: Duration = Duration::from_secs(20);
const ABI_PROBE_TIMEOUT: Duration = Duration::from_secs(180);

pub fn run(args: impl Iterator<Item = String>) -> Result<String, String> {
    let args = parse_args(args)?;
    let profile = pandar_studio_profile::profile(&args.studio_profile)?;
    let target = validate_current_host(&args.label)?;
    let archive_sha256 = validate_checksum(&args.archive, &args.checksum)?;
    let stage = stage_archive(
        &args.archive,
        &args.cli_name,
        &args.plugin_name,
        &args.source_name,
    )?;
    let expected = expected_symbols(&args.repo_root, profile)?;
    let inspection = inspect_exports(target, &stage.plugin)?;
    validate_exact_exports(&expected.all, &inspection.symbols)?;
    let source_inspection = inspect_source_exports(target, &stage.source)?;
    validate_source_exports(&source_inspection.symbols)?;
    let source_sha256 = sha256_hex(&stage.source)?;
    run_packaged_cli(&stage.cli, CLI_TIMEOUT)?;
    let mut abi_probe_args = vec!["--studio-profile".to_owned(), profile.id.clone()];
    abi_probe_args.extend(args.abi_probe_args);
    let probe = run_abi_probe(
        &args.abi_probe,
        &abi_probe_args,
        &stage.plugin,
        ABI_PROBE_TIMEOUT,
    )?;
    collect_evidence(EvidenceInput {
        target,
        studio_profile: &profile.id,
        archive_sha256: &archive_sha256,
        plugin_sha256: &probe.plugin_sha256,
        source_sha256: &source_sha256,
        network_symbols: expected.network_count,
        file_transfer_symbols: expected.file_transfer_count,
        plugin_inspector: inspection.inspector,
        source_inspector: source_inspection.inspector,
        plugin: &stage.plugin,
        source_sentinel: SOURCE_SENTINEL,
    })
}
