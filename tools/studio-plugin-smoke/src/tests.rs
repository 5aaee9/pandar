use super::*;
use std::{fs, io::Write};

fn temp_file(dir: &tempfile::TempDir, name: &str) -> PathBuf {
    let path = dir.path().join(name);
    let mut file = fs::File::create(&path).unwrap();
    writeln!(file, "placeholder").unwrap();
    path
}

fn valid_args(dir: &tempfile::TempDir) -> Args {
    Args {
        studio_path: temp_file(dir, "BambuStudio"),
        plugin_artifact: temp_file(dir, "libpandar_network_plugin.so"),
        hub_url: "https://hub.example".to_owned(),
        frontend_url: "https://web.example".to_owned(),
        os: TargetOs::Linux,
        arch: "x86_64".to_owned(),
        studio_version: "1.10.2".to_owned(),
        test_date: "2026-06-24".to_owned(),
        pandar_commit: "abcdef123456".to_owned(),
    }
}

fn valid_cli_args(dir: &tempfile::TempDir) -> Vec<String> {
    let args = valid_args(dir);
    vec![
        "--preflight".to_owned(),
        "--studio-path".to_owned(),
        args.studio_path.display().to_string(),
        "--plugin-artifact".to_owned(),
        args.plugin_artifact.display().to_string(),
        "--hub-url".to_owned(),
        args.hub_url,
        "--frontend-url".to_owned(),
        args.frontend_url,
        "--os".to_owned(),
        args.os.as_str().to_owned(),
        "--arch".to_owned(),
        args.arch,
        "--studio-version".to_owned(),
        args.studio_version,
        "--test-date".to_owned(),
        args.test_date,
        "--pandar-commit".to_owned(),
        args.pandar_commit,
    ]
}

#[test]
fn valid_linux_preflight_renders_pass_and_evidence_row() {
    let dir = tempfile::tempdir().unwrap();
    let args = valid_args(&dir);

    validate(&args).unwrap();
    let output = render_output(&args);

    assert!(output.contains("PASS studio-plugin-preflight"));
    assert!(output.contains("target: os=linux arch=x86_64 studio_version=1.10.2"));
    assert!(output.contains("plugin_artifact: libpandar_network_plugin.so"));
    assert!(output.contains(
        "| 1.10.2 | linux | x86_64 | `libpandar_network_plugin.so` | `abcdef123456` | 2026-06-24 |"
    ));
}

#[test]
fn linux_target_rejects_windows_plugin_filename() {
    let dir = tempfile::tempdir().unwrap();
    let mut args = valid_args(&dir);
    args.plugin_artifact = temp_file(&dir, "pandar_network_plugin.dll");

    let error = validate(&args).unwrap_err();

    assert!(error.contains("pandar_network_plugin.dll"));
    assert!(error.contains("expected libpandar_network_plugin.so"));
}

#[test]
fn windows_target_rejects_linux_plugin_filename() {
    let dir = tempfile::tempdir().unwrap();
    let mut args = valid_args(&dir);
    args.os = TargetOs::Windows;

    let error = validate(&args).unwrap_err();

    assert!(error.contains("libpandar_network_plugin.so"));
    assert!(error.contains("expected pandar_network_plugin.dll"));
}

#[test]
fn macos_target_rejects_windows_plugin_filename() {
    let dir = tempfile::tempdir().unwrap();
    let mut args = valid_args(&dir);
    args.os = TargetOs::Macos;
    args.plugin_artifact = temp_file(&dir, "pandar_network_plugin.dll");

    let error = validate(&args).unwrap_err();

    assert!(error.contains("pandar_network_plugin.dll"));
    assert!(error.contains("expected libpandar_network_plugin.dylib"));
}

#[test]
fn missing_studio_path_and_plugin_artifact_are_reported_together() {
    let dir = tempfile::tempdir().unwrap();
    let mut args = valid_args(&dir);
    args.studio_path = dir.path().join("missing-studio");
    args.plugin_artifact = dir
        .path()
        .join("missing")
        .join("libpandar_network_plugin.so");

    let error = validate(&args).unwrap_err();

    assert!(error.contains("- studio path does not exist"));
    assert!(error.contains("- plugin artifact does not exist"));
}

#[test]
fn plugin_artifact_directory_path_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut args = valid_args(&dir);
    let artifact_dir = dir
        .path()
        .join("nested")
        .join("libpandar_network_plugin.so");
    fs::create_dir(dir.path().join("nested")).unwrap();
    fs::create_dir(&artifact_dir).unwrap();
    args.plugin_artifact = artifact_dir;

    let error = validate(&args).unwrap_err();

    assert!(error.contains("plugin artifact is not a file"));
}

#[test]
fn malformed_url_scheme_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut args = valid_args(&dir);
    args.hub_url = "ftp://hub.example".to_owned();

    let error = validate(&args).unwrap_err();

    assert!(error.contains("hub-url must be an absolute http(s) URL"));
}

#[test]
fn url_credentials_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut args = valid_args(&dir);
    args.hub_url = "https://user:pass@hub.example".to_owned();

    let error = validate(&args).unwrap_err();

    assert!(error.contains("hub-url must not include credentials"));
}

#[test]
fn missing_required_cli_flag_is_rejected_by_parse_args() {
    let dir = tempfile::tempdir().unwrap();
    let mut raw_args = valid_cli_args(&dir);
    let flag_position = raw_args
        .iter()
        .position(|arg| arg == "--frontend-url")
        .unwrap();
    raw_args.drain(flag_position..=flag_position + 1);

    let error = parse_args(raw_args).unwrap_err();

    assert!(error.contains("missing required flag --frontend-url"));
}

#[test]
fn commit_values_with_slash_backslash_or_whitespace_are_rejected() {
    let dir = tempfile::tempdir().unwrap();

    for commit in ["abc/def", "abc\\def", "abc def", "abc\tdef"] {
        let mut args = valid_args(&dir);
        args.pandar_commit = commit.to_owned();

        let error = validate(&args).unwrap_err();

        assert!(error.contains("pandar-commit must not contain slashes or whitespace"));
    }
}

#[test]
fn invalid_date_shape_is_rejected() {
    let dir = tempfile::tempdir().unwrap();

    for test_date in ["2026-6-24", "20260624", "2026-aa-24", "2026-06-240"] {
        let mut args = valid_args(&dir);
        args.test_date = test_date.to_owned();

        let error = validate(&args).unwrap_err();

        assert!(error.contains("test-date must match YYYY-MM-DD"));
    }
}

#[test]
fn evidence_row_has_17_cells_and_10_untested_status_values() {
    let dir = tempfile::tempdir().unwrap();
    let args = valid_args(&dir);
    let output = render_output(&args);
    let row = output.lines().last().unwrap();
    let cells: Vec<&str> = row.trim_matches('|').split('|').map(str::trim).collect();

    assert_eq!(cells.len(), 17);
    assert_eq!(
        cells.iter().filter(|cell| **cell == "`untested`").count(),
        10
    );
}

#[test]
fn valid_preflight_output_includes_explicit_test_date_in_evidence_row() {
    let dir = tempfile::tempdir().unwrap();
    let mut raw_args = valid_cli_args(&dir);
    let date_position = raw_args
        .iter()
        .position(|arg| arg == "--test-date")
        .unwrap()
        + 1;
    raw_args[date_position] = "2030-01-02".to_owned();

    let args = parse_args(raw_args).unwrap();
    validate(&args).unwrap();
    let output = render_output(&args);

    assert!(output.lines().last().unwrap().contains("2030-01-02"));
}

#[test]
fn plugin_filename_mismatch_reports_filenames_without_full_path() {
    let dir = tempfile::tempdir().unwrap();
    let mut args = valid_args(&dir);
    args.plugin_artifact = temp_file(&dir, "pandar_network_plugin.dll");

    let error = validate(&args).unwrap_err();

    assert!(error.contains("pandar_network_plugin.dll"));
    assert!(error.contains("expected libpandar_network_plugin.so"));
    assert!(!error.contains(dir.path().to_str().unwrap()));
}
