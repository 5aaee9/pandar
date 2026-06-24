use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Debug, Eq, PartialEq)]
struct Args {
    studio_path: PathBuf,
    plugin_artifact: PathBuf,
    hub_url: String,
    frontend_url: String,
    os: TargetOs,
    arch: String,
    studio_version: String,
    test_date: String,
    pandar_commit: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetOs {
    Linux,
    Windows,
    Macos,
}

impl TargetOs {
    fn as_str(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Windows => "windows",
            Self::Macos => "macos",
        }
    }

    fn expected_plugin_filename(self) -> &'static str {
        match self {
            Self::Linux => "libpandar_network_plugin.so",
            Self::Windows => "pandar_network_plugin.dll",
            Self::Macos => "libpandar_network_plugin.dylib",
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<String, String> {
    let args = parse_args(env::args().skip(1).collect())?;
    validate(&args)?;
    Ok(render_output(&args))
}

fn parse_args(raw_args: Vec<String>) -> Result<Args, String> {
    let mut issues = Vec::new();
    if raw_args.first().map(String::as_str) != Some("--preflight") {
        issues.push("first argument must be --preflight".to_owned());
    }

    let mut values = BTreeMap::new();
    let mut position = if raw_args.first().map(String::as_str) == Some("--preflight") {
        1
    } else {
        0
    };
    while position < raw_args.len() {
        let flag = &raw_args[position];
        if !flag.starts_with("--") {
            issues.push("expected --name flag after --preflight".to_owned());
            position += 1;
            continue;
        }
        if flag == "--preflight" {
            issues.push("--preflight must appear only as the leading mode token".to_owned());
            position += 1;
            continue;
        }
        let Some(value) = raw_args.get(position + 1) else {
            issues.push(format!("{flag} requires a value"));
            break;
        };
        if value.starts_with("--") {
            issues.push(format!("{flag} requires a value"));
            position += 1;
            continue;
        }
        if values.insert(flag.clone(), value.clone()).is_some() {
            issues.push(format!("{flag} was provided more than once"));
        }
        position += 2;
    }

    let required = [
        "--studio-path",
        "--plugin-artifact",
        "--hub-url",
        "--frontend-url",
        "--os",
        "--arch",
        "--studio-version",
        "--test-date",
        "--pandar-commit",
    ];
    for flag in required {
        if !values.contains_key(flag) {
            issues.push(format!("missing required flag {flag}"));
        }
    }

    let allowed: BTreeSet<&str> = required.into_iter().collect();
    for flag in values.keys() {
        if !allowed.contains(flag.as_str()) {
            issues.push(format!("unknown flag {flag}"));
        }
    }

    let os = values
        .get("--os")
        .and_then(|value| parse_os(value, &mut issues));

    if !issues.is_empty() {
        return Err(format_issues(issues));
    }

    Ok(Args {
        studio_path: PathBuf::from(values.remove("--studio-path").unwrap()),
        plugin_artifact: PathBuf::from(values.remove("--plugin-artifact").unwrap()),
        hub_url: values.remove("--hub-url").unwrap(),
        frontend_url: values.remove("--frontend-url").unwrap(),
        os: os.unwrap(),
        arch: values.remove("--arch").unwrap(),
        studio_version: values.remove("--studio-version").unwrap(),
        test_date: values.remove("--test-date").unwrap(),
        pandar_commit: values.remove("--pandar-commit").unwrap(),
    })
}

fn parse_os(value: &str, issues: &mut Vec<String>) -> Option<TargetOs> {
    match value {
        "linux" => Some(TargetOs::Linux),
        "windows" => Some(TargetOs::Windows),
        "macos" => Some(TargetOs::Macos),
        _ => {
            issues.push(format!("unsupported os {value}"));
            None
        }
    }
}

fn validate(args: &Args) -> Result<(), String> {
    let mut issues = Vec::new();

    if !args.studio_path.exists() {
        issues.push("studio path does not exist".to_owned());
    }

    if !args.plugin_artifact.exists() {
        issues.push("plugin artifact does not exist".to_owned());
    } else if !args.plugin_artifact.is_file() {
        issues.push("plugin artifact is not a file".to_owned());
    }

    let actual_filename = plugin_filename(&args.plugin_artifact);
    let expected_filename = args.os.expected_plugin_filename();
    if actual_filename.as_deref() != Some(expected_filename) {
        issues.push(format!(
            "plugin artifact filename {} does not match {} target; expected {expected_filename}",
            actual_filename.as_deref().unwrap_or("<missing-filename>"),
            args.os.as_str()
        ));
    }

    validate_url("hub-url", &args.hub_url, &mut issues);
    validate_url("frontend-url", &args.frontend_url, &mut issues);

    if !matches!(args.arch.as_str(), "x86_64" | "amd64" | "aarch64" | "arm64") {
        issues.push(format!("unsupported arch {}", args.arch));
    }

    if args.studio_version.is_empty() {
        issues.push("studio-version must be non-empty".to_owned());
    }

    if !valid_date_shape(&args.test_date) {
        issues.push("test-date must match YYYY-MM-DD".to_owned());
    }

    if args.pandar_commit.is_empty() {
        issues.push("pandar-commit must be non-empty".to_owned());
    } else if args
        .pandar_commit
        .chars()
        .any(|character| matches!(character, '/' | '\\') || character.is_ascii_whitespace())
    {
        issues.push("pandar-commit must not contain slashes or whitespace".to_owned());
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(format_issues(issues))
    }
}

fn validate_url(label: &str, value: &str, issues: &mut Vec<String>) {
    let Some(authority_start) = value
        .strip_prefix("http://")
        .or_else(|| value.strip_prefix("https://"))
    else {
        issues.push(format!("{label} must be an absolute http(s) URL"));
        return;
    };

    let authority = authority_start.split('/').next().unwrap_or_default();
    if authority.is_empty() {
        issues.push(format!("{label} must include a host"));
    }
    if authority.contains('@') {
        issues.push(format!("{label} must not include credentials"));
    }
}

fn valid_date_shape(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit)
}

fn render_output(args: &Args) -> String {
    let filename =
        plugin_filename(&args.plugin_artifact).unwrap_or_else(|| "<missing-filename>".to_owned());
    let row = render_evidence_row(args, &filename);
    format!(
        "PASS studio-plugin-preflight\n\
         target: os={} arch={} studio_version={}\n\
         plugin_artifact: {filename}\n\
         pandar_commit: {}\n\
         evidence_row:\n\
         {row}",
        args.os.as_str(),
        args.arch,
        args.studio_version,
        args.pandar_commit
    )
}

fn render_evidence_row(args: &Args, filename: &str) -> String {
    format!(
        "| {} | {} | {} | `{}` | `{}` | {} | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | Preflight passed only; replace this evidence note after a real Studio run. |",
        args.studio_version,
        args.os.as_str(),
        args.arch,
        filename,
        args.pandar_commit,
        args.test_date
    )
}

fn plugin_filename(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|filename| filename.to_str())
        .map(str::to_owned)
}

fn format_issues(issues: Vec<String>) -> String {
    let mut output = String::from("studio plugin preflight failed:");
    for issue in issues {
        output.push_str("\n- ");
        output.push_str(&issue);
    }
    output
}

#[cfg(test)]
mod tests;
