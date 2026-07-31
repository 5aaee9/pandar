use std::path::PathBuf;

#[derive(Debug)]
pub(crate) struct Args {
    pub label: String,
    pub archive: PathBuf,
    pub checksum: PathBuf,
    pub cli_name: String,
    pub plugin_name: String,
    pub source_name: String,
    pub repo_root: PathBuf,
    pub abi_probe: PathBuf,
    pub abi_probe_args: Vec<String>,
}

pub(crate) fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut label = None;
    let mut archive = None;
    let mut checksum = None;
    let mut cli_name = None;
    let mut plugin_name = None;
    let mut source_name = None;
    let mut repo_root = None;
    let mut abi_probe = None;
    let mut abi_probe_args = Vec::new();
    let mut args = args;

    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("{flag} requires a value\n{}", usage()))?;
        match flag.as_str() {
            "--label" if label.is_none() => label = Some(value),
            "--archive" if archive.is_none() => archive = Some(PathBuf::from(value)),
            "--checksum" if checksum.is_none() => checksum = Some(PathBuf::from(value)),
            "--cli-name" if cli_name.is_none() => cli_name = Some(value),
            "--plugin-name" if plugin_name.is_none() => plugin_name = Some(value),
            "--source-name" if source_name.is_none() => source_name = Some(value),
            "--repo-root" if repo_root.is_none() => repo_root = Some(PathBuf::from(value)),
            "--abi-probe" if abi_probe.is_none() => abi_probe = Some(PathBuf::from(value)),
            "--abi-probe-arg" => {
                if value == "--plugin" || value.starts_with("--plugin=") {
                    return Err(
                        "--plugin is reserved; release-smoke always supplies the staged plugin"
                            .to_owned(),
                    );
                }
                abi_probe_args.push(value);
            }
            "--label" | "--archive" | "--checksum" | "--cli-name" | "--plugin-name"
            | "--source-name" | "--repo-root" | "--abi-probe" => {
                return Err(format!("{flag} was provided twice"));
            }
            _ => return Err(format!("unknown argument {flag}\n{}", usage())),
        }
    }

    Ok(Args {
        label: label.ok_or_else(usage)?,
        archive: archive.ok_or_else(usage)?,
        checksum: checksum.ok_or_else(usage)?,
        cli_name: cli_name.ok_or_else(usage)?,
        plugin_name: plugin_name.ok_or_else(usage)?,
        source_name: source_name.ok_or_else(usage)?,
        repo_root: repo_root.ok_or_else(usage)?,
        abi_probe: abi_probe.ok_or_else(usage)?,
        abi_probe_args,
    })
}

fn usage() -> String {
    concat!(
        "usage: pandar-release-smoke ",
        "--label <linux-amd64|macos-amd64|macos-arm64|windows-amd64> ",
        "--archive <path> --checksum <path> ",
        "--cli-name <filename> --plugin-name <filename> --source-name <filename> ",
        "--repo-root <path> --abi-probe <native-executable> ",
        "[--abi-probe-arg <value>]..."
    )
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::parse_args;

    fn required_args_without_source() -> Vec<String> {
        [
            "--label",
            "windows-amd64",
            "--archive",
            "release.tar.gz",
            "--checksum",
            "release.tar.gz.sha256",
            "--cli-name",
            "pandar.exe",
            "--plugin-name",
            "pandar_network_plugin.dll",
            "--repo-root",
            ".",
            "--abi-probe",
            "probe.exe",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    fn required_args() -> Vec<String> {
        let mut args = required_args_without_source();
        args.extend([
            "--source-name".to_owned(),
            "pandar_bambu_source.dll".to_owned(),
        ]);
        args
    }

    #[test]
    fn source_artifact_name_is_required() {
        assert!(
            parse_args(required_args_without_source().into_iter())
                .unwrap_err()
                .contains("--source-name")
        );
        assert_eq!(
            parse_args(required_args().into_iter()).unwrap().source_name,
            "pandar_bambu_source.dll"
        );
    }

    #[test]
    fn runner_os_is_not_a_caller_controlled_argument() {
        let mut args = required_args();
        args.extend(["--runner-os".to_owned(), "windows".to_owned()]);

        assert!(
            parse_args(args.into_iter())
                .unwrap_err()
                .contains("unknown argument")
        );
    }

    #[test]
    fn plugin_argument_is_reserved_for_the_staged_artifact() {
        let mut args = required_args();
        args.extend(["--abi-probe-arg".to_owned(), "--plugin".to_owned()]);

        assert!(
            parse_args(args.into_iter())
                .unwrap_err()
                .contains("reserved")
        );
    }
}
