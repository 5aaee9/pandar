mod boost;
mod http_probe;
mod native;
mod plugin;
mod source;
mod source_mapping;
mod types;

use std::{env, path::PathBuf, process::ExitCode};

use native::{NativeReport, verify_native_contract};
use plugin::{verify_exports, verify_required_exports};
use source::{PINNED_BOOST_VERSION, inspect_source};
use types::verify_pandar_abi_contract;

fn main() -> ExitCode {
    match run(env::args().skip(1)) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: impl Iterator<Item = String>) -> Result<String, String> {
    let args = parse_args(args)?;
    let abi_series = pandar_studio_profile::abi_series(&args.studio_abi_series)?;
    let contract = inspect_source(&args.source, abi_series)?;
    let mut failures = Vec::new();
    if args.scope == Scope::Full
        && let Err(error) = verify_pandar_abi_contract(&contract, abi_series)
    {
        failures.push(error);
    }
    let export_result = match args.scope {
        Scope::Full => verify_exports(&args.plugin, &contract),
        Scope::FtSafety => verify_required_exports(&args.plugin, &contract.file_transfer_symbols),
    };
    let export_count = match export_result {
        Ok(report) => {
            if !report.missing.is_empty() {
                failures.push(format!(
                    "plugin is missing symbols loaded by pinned Bambu Studio: {}",
                    report.missing.join(", ")
                ));
            }
            report.count
        }
        Err(error) => {
            failures.push(error);
            0
        }
    };
    let modes = match args.scope {
        Scope::Full => abi_series.native_modes(),
        Scope::FtSafety => &["ft"],
    };
    let native = verify_native_contract(
        &args.source,
        &args.plugin,
        &args.boost_archive,
        modes,
        args.scope == Scope::Full,
        args.address_sanitizer,
        abi_series,
    )?;
    let metadata = metadata(&contract, abi_series, export_count, &native, args.scope);
    failures.extend(native.failures);
    if !failures.is_empty() {
        return Err(format!(
            "{metadata}\ncontract_status=failed\nfailures:\n- {}",
            failures.join("\n- ")
        ));
    }
    Ok(format!("{metadata}\ncontract_status=passed"))
}

fn metadata(
    contract: &source::StudioContract,
    abi_series: &pandar_studio_profile::StudioAbiSeries,
    export_count: usize,
    native: &NativeReport,
    scope: Scope,
) -> String {
    format!(
        "contract_scope={}\nabi_series={}\nstudio_commit={}\nstudio_version={}\nreference_network_agent_version={}\nreported_network_agent_version={}\nboost_version={}\nboost_sha256={}\nnetwork_symbols={}\nfile_transfer_symbols={}\nplugin_exports={export_count}\ncompiler={}\nnative_modes={}",
        scope.as_str(),
        abi_series.id,
        contract.commit,
        contract.studio_version,
        contract.reference_network_agent_version,
        abi_series.reported_network_agent_version,
        PINNED_BOOST_VERSION,
        native.boost_sha256,
        contract.network_symbols.len(),
        contract.file_transfer_symbols.len(),
        native.compiler,
        native.passed_modes.join(",")
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Scope {
    Full,
    FtSafety,
}

impl Scope {
    fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::FtSafety => "ft-safety",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct Args {
    studio_abi_series: String,
    source: PathBuf,
    plugin: PathBuf,
    boost_archive: PathBuf,
    scope: Scope,
    address_sanitizer: bool,
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut source = None;
    let mut studio_abi_series = None;
    let mut plugin = None;
    let mut boost_archive = None;
    let mut scope = Scope::Full;
    let mut address_sanitizer = false;
    let mut args = args;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--ft-safety-only" if scope == Scope::Full => {
                scope = Scope::FtSafety;
                continue;
            }
            "--ft-safety-only" => return Err("--ft-safety-only was provided twice".to_owned()),
            "--address-sanitizer" if !address_sanitizer => {
                address_sanitizer = true;
                continue;
            }
            "--address-sanitizer" => {
                return Err("--address-sanitizer was provided twice".to_owned());
            }
            _ => {}
        }
        let value = args
            .next()
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag.as_str() {
            "--studio-abi-series" if studio_abi_series.is_none() => studio_abi_series = Some(value),
            "--studio-source" if source.is_none() => source = Some(PathBuf::from(value)),
            "--plugin" if plugin.is_none() => plugin = Some(PathBuf::from(value)),
            "--boost-archive" if boost_archive.is_none() => {
                boost_archive = Some(PathBuf::from(value));
            }
            "--studio-abi-series" | "--studio-source" | "--plugin" | "--boost-archive" => {
                return Err(format!("{flag} was provided twice"));
            }
            _ => return Err(format!("unknown argument {flag}")),
        }
    }
    let args = Args {
        studio_abi_series: studio_abi_series.ok_or("missing --studio-abi-series <MM.mm.pp>")?,
        source: source.ok_or("missing --studio-source <official-checkout>")?,
        plugin: plugin.ok_or("missing --plugin <native-library>")?,
        boost_archive: boost_archive.ok_or("missing --boost-archive <boost-1.84.0.tar.gz>")?,
        scope,
        address_sanitizer,
    };
    if args.address_sanitizer && args.scope != Scope::FtSafety {
        return Err("--address-sanitizer requires --ft-safety-only".to_owned());
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::{Args, Scope, parse_args};
    use std::path::PathBuf;

    #[test]
    fn parses_explicit_dependency_and_ft_safety_scope() {
        let args = parse_args(
            [
                "--studio-abi-series",
                "02.07.01",
                "--studio-source",
                "studio",
                "--plugin",
                "plugin.so",
                "--boost-archive",
                "boost-1.84.0.tar.gz",
                "--ft-safety-only",
                "--address-sanitizer",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .unwrap();

        assert_eq!(
            args,
            Args {
                studio_abi_series: "02.07.01".to_owned(),
                source: PathBuf::from("studio"),
                plugin: PathBuf::from("plugin.so"),
                boost_archive: PathBuf::from("boost-1.84.0.tar.gz"),
                scope: Scope::FtSafety,
                address_sanitizer: true,
            }
        );
    }

    #[test]
    fn rejects_address_sanitizer_outside_ft_safety_scope() {
        let error = parse_args(
            [
                "--studio-abi-series",
                "02.07.01",
                "--studio-source",
                "studio",
                "--plugin",
                "plugin.so",
                "--boost-archive",
                "boost-1.84.0.tar.gz",
                "--address-sanitizer",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .unwrap_err();

        assert!(error.contains("--ft-safety-only"));
    }

    #[test]
    fn requires_boost_archive_and_rejects_duplicates() {
        let missing = parse_args(
            [
                "--studio-abi-series",
                "02.07.01",
                "--studio-source",
                "studio",
                "--plugin",
                "plugin.so",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .unwrap_err();
        assert!(
            missing.contains("missing --boost-archive"),
            "unexpected error: {missing}"
        );

        let duplicate = parse_args(
            [
                "--studio-abi-series",
                "02.07.01",
                "--studio-source",
                "studio",
                "--plugin",
                "plugin.so",
                "--boost-archive",
                "first.tar.gz",
                "--boost-archive",
                "second.tar.gz",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .unwrap_err();
        assert!(
            duplicate.contains("--boost-archive was provided twice"),
            "unexpected error: {duplicate}"
        );
    }
}
