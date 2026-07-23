use std::{fs, path::PathBuf, process::Command, time::Duration};

use tempfile::tempdir;

use super::{run_abi_probe, run_packaged_cli};

#[test]
fn probe_receives_absolute_staged_plugin_instead_of_decoy() {
    let temp = tempdir().unwrap();
    let helper = compile_probe_helper(temp.path());
    let plugin = temp.path().join("packaged.plugin");
    let decoy = temp.path().join("decoy.plugin");
    fs::write(&plugin, b"packaged").unwrap();
    fs::write(&decoy, b"decoy").unwrap();

    let report = run_abi_probe(
        &helper,
        &[
            "verify".to_owned(),
            "--decoy".to_owned(),
            decoy.display().to_string(),
        ],
        &plugin,
        Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(report.plugin_sha256.len(), 64);
}

#[test]
fn probe_nonzero_exit_and_timeout_are_failures() {
    let temp = tempdir().unwrap();
    let helper = compile_probe_helper(temp.path());
    let plugin = temp.path().join("packaged.plugin");
    fs::write(&plugin, b"packaged").unwrap();

    let nonzero = run_abi_probe(
        &helper,
        &["nonzero".to_owned()],
        &plugin,
        Duration::from_secs(5),
    )
    .unwrap_err();
    assert!(nonzero.contains("exited"));

    let timeout = run_abi_probe(
        &helper,
        &["timeout".to_owned()],
        &plugin,
        Duration::from_millis(100),
    )
    .unwrap_err();
    assert!(timeout.contains("timed out"));
}

#[test]
fn probe_hash_mutation_is_a_failure() {
    let temp = tempdir().unwrap();
    let helper = compile_probe_helper(temp.path());
    let plugin = temp.path().join("packaged.plugin");
    fs::write(&plugin, b"packaged").unwrap();

    let error = run_abi_probe(
        &helper,
        &["mutate".to_owned()],
        &plugin,
        Duration::from_secs(5),
    )
    .unwrap_err();

    assert!(error.contains("mutated"));
}

#[test]
fn probe_failure_preserves_diagnostics_but_redacts_args_and_stage_path() {
    let temp = tempdir().unwrap();
    let helper = compile_probe_helper(temp.path());
    let plugin = temp.path().join("packaged.plugin");
    fs::write(&plugin, b"packaged").unwrap();
    let token = "super-secret-token";

    let error = run_abi_probe(
        &helper,
        &["leak".to_owned(), token.to_owned()],
        &plugin,
        Duration::from_secs(5),
    )
    .unwrap_err();

    assert!(error.contains("probe diagnostic"));
    assert!(!error.contains(token));
    assert!(!error.contains(&plugin.display().to_string()));
}

#[test]
fn packaged_cli_runs_help_and_enforces_timeout() {
    let temp = tempdir().unwrap();
    let helper = compile_probe_helper(temp.path());
    run_packaged_cli(&helper, Duration::from_secs(5)).unwrap();

    let sleeping = compile_sleeping_cli(temp.path());
    let error = run_packaged_cli(&sleeping, Duration::from_millis(100)).unwrap_err();
    assert!(error.contains("timed out"));
}

fn compile_probe_helper(root: &std::path::Path) -> PathBuf {
    let source = root.join("probe.rs");
    fs::write(
        &source,
        r#"
use std::{env, fs, path::Path, process, thread, time::Duration};

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some("--help") {
        return;
    }
    if args.first().map(String::as_str) == Some("nonzero") {
        process::exit(7);
    }
    if args.first().map(String::as_str) == Some("timeout") {
        thread::sleep(Duration::from_secs(30));
        return;
    }
    let plugin = args
        .windows(2)
        .find(|pair| pair[0] == "--plugin")
        .map(|pair| Path::new(&pair[1]))
        .unwrap_or_else(|| process::exit(8));
    if !plugin.is_absolute() {
        process::exit(9);
    }
    if args.first().map(String::as_str) == Some("leak") {
        eprintln!("probe diagnostic {} {}", args[1], plugin.display());
        process::exit(11);
    }
    match args.first().map(String::as_str) {
        Some("verify") if fs::read(plugin).ok().as_deref() == Some(b"packaged") => {}
        Some("mutate") => fs::write(plugin, b"mutated").unwrap(),
        _ => process::exit(10),
    }
}
"#,
    )
    .unwrap();
    let executable = root.join(if cfg!(windows) {
        "probe-helper.exe"
    } else {
        "probe-helper"
    });
    let output = Command::new("rustc")
        .args(["--edition=2024"])
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "compile probe helper: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    executable
}

fn compile_sleeping_cli(root: &std::path::Path) -> PathBuf {
    let source = root.join("sleeping_cli.rs");
    fs::write(
        &source,
        "fn main() { std::thread::sleep(std::time::Duration::from_secs(30)); }",
    )
    .unwrap();
    let executable = root.join(if cfg!(windows) {
        "sleeping-cli.exe"
    } else {
        "sleeping-cli"
    });
    let output = Command::new("rustc")
        .args(["--edition=2024"])
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(output.status.success());
    executable
}
