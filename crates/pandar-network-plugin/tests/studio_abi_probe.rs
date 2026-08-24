#![cfg(any(unix, windows))]
#![recursion_limit = "256"]

#[path = "studio_abi_probe/compiler.rs"]
mod compiler;
#[path = "studio_abi_probe/disposition_probe.rs"]
mod disposition_probe;
#[path = "studio_abi_probe/firmware_mock.rs"]
mod firmware_mock;
#[path = "studio_abi_probe/firmware_probe.rs"]
mod firmware_probe;
#[path = "studio_abi_probe/mock_hub.rs"]
mod mock_hub;
#[path = "studio_abi_probe/native_print_error.rs"]
mod native_print_error;
#[path = "studio_abi_probe/request_claims.rs"]
mod request_claims;
#[path = "studio_abi_probe/run.rs"]
mod run;
mod support;

use mock_hub::MockMode;
use run::{ProbeOutput, run_probe};

const EXPECTED_STALE_REFRESH_DIAGNOSTIC: &str =
    "pandar printer status refresh discarded: credentials changed during request";

fn assert_json_field(output: &str, field: &str, value: &str) {
    assert!(
        output.contains(&format!(r#""{field}":{value}"#)),
        "probe output missing {field}={value}: {output}"
    );
}

fn compiler_identity_is_allowed_for_target(compiler: &str, target_requires_msvc: bool) -> bool {
    !target_requires_msvc || compiler.to_ascii_lowercase().contains("cl.exe")
}

fn assert_only_expected_stale_refresh_diagnostics(stderr: &str) {
    assert!(
        stderr.is_empty()
            || stderr
                .lines()
                .all(|line| line == EXPECTED_STALE_REFRESH_DIAGNOSTIC),
        "firmware ABI probe wrote unexpected stderr: {stderr}"
    );
}

#[test]
fn firmware_probe_wires_native_cloud_and_lan_behavior() {
    let output = firmware_probe::run_firmware_probe();
    println!("Studio firmware ABI probe compiler: {}", output.compiler);
    #[cfg(all(windows, target_env = "msvc"))]
    assert!(
        compiler_identity_is_allowed_for_target(&output.compiler, true),
        "firmware ABI probe must compile with MSVC cl.exe, got {}",
        output.compiler
    );
    assert_only_expected_stale_refresh_diagnostics(&output.stderr);
    let result: serde_json::Value = serde_json::from_str(output.stdout.trim()).unwrap();
    assert_eq!(result["ok"], serde_json::json!(true));
    assert_eq!(result["catalog_exact"], serde_json::json!(true));
    assert_eq!(result["versions_exact"], serde_json::json!(true));
    assert!(result["callback_delay_ms"].as_u64().unwrap() >= 1_000);
    assert!(result["callback_delay_ms"].as_u64().unwrap() < 2_000);
    assert!(result["overlap_callback_delay_ms"].as_u64().unwrap() >= 1_000);
    assert!(result["overlap_callback_delay_ms"].as_u64().unwrap() < 2_000);
    assert_eq!(result["overlap_callback_exact"], serde_json::json!(true));
    assert_eq!(result["callbacks_serialized"], serde_json::json!(true));
    assert_eq!(result["status_logout_safe"], serde_json::json!(true));
    assert_eq!(
        result["synchronous_generation_fenced"],
        serde_json::json!(true)
    );
    assert_eq!(
        result["synchronous_reentrant_logout"],
        serde_json::json!(true)
    );
    assert_eq!(result["deadline_expired"], serde_json::json!(true));
    assert_eq!(result["logout_cancelled"], serde_json::json!(true));
    assert_eq!(result["destroy_cancelled"], serde_json::json!(true));
}

#[test]
fn probe_exercises_studio_abi_success_path() {
    let ProbeOutput { stdout, stderr, .. } = run_probe(MockMode::Success, "success");

    assert!(stderr.is_empty(), "probe stderr was not empty: {stderr}");
    assert_json_field(&stdout, "ok", "true");
    assert!(
        stdout.contains(r#""host":"http://127.0.0.1:"#),
        "probe host did not use local webserver: {stdout}"
    );
    assert!(stdout.contains("studio_userlogin"));
    assert!(stdout.contains("studio_useroffline"));
    assert_json_field(&stdout, "printer_rc", "0");
    assert_json_field(&stdout, "tasks_rc", "0");
    assert_json_field(&stdout, "print_rc", "0");
    assert_json_field(&stdout, "update_stage", "6");
    assert_json_field(&stdout, "update_code", "0");
    assert!(stdout.contains(r#""update_body":"3""#));
    assert_json_field(&stdout, "restored_login", "true");
    assert_json_field(&stdout, "ft_abi_version", "1");
    assert_json_field(&stdout, "ft_start_connect_rc", "0");
    assert_json_field(&stdout, "ft_sync_rc", "-3");
    assert_json_field(&stdout, "ft_start_job_rc", "0");
    assert_json_field(&stdout, "ft_job_result_ec", "-3");
    assert_json_field(&stdout, "ft_cancel_rc", "0");
    assert_json_field(&stdout, "camera_unavailable_exact", "true");
    assert_json_field(&stdout, "local_connect_transport_redacted", "true");
}

#[test]
fn compiled_probe_enforces_account_callbacks_and_explicit_abi_dispositions() {
    let output = disposition_probe::run_disposition_probe();
    println!("Studio disposition ABI probe compiler: {}", output.compiler);
    #[cfg(all(windows, target_env = "msvc"))]
    assert!(
        compiler_identity_is_allowed_for_target(&output.compiler, true),
        "disposition ABI probe must compile with MSVC cl.exe, got {}",
        output.compiler
    );
    assert!(
        output.stderr.is_empty(),
        "disposition ABI probe stderr was not empty: {}",
        output.stderr
    );
    assert_eq!(output.stdout.trim(), r#"{"ok":true,"version":1}"#);
}

#[test]
fn probe_camera_abis_fail_closed_even_when_agent_has_printer_credentials() {
    let ProbeOutput { stdout, stderr, .. } =
        run_probe(MockMode::CameraUnavailable, "camera-unavailable");

    assert!(
        stderr.is_empty(),
        "camera unavailable probe stderr was not empty: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
    assert_json_field(&stdout, "camera_unavailable_exact", "true");
}

#[test]
fn probe_camera_abis_return_one_use_loopback_urls_for_verified_models() {
    let ProbeOutput { stdout, stderr, .. } =
        run_probe(MockMode::CameraAvailable, "camera-available");

    assert!(
        stderr.is_empty(),
        "camera available probe stderr was not empty: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
    assert_json_field(&stdout, "camera_available_exact", "true");
    assert_json_field(&stdout, "camera_callback_count", "4");
    assert_json_field(&stdout, "camera_golive_callback_count", "4");
    assert_json_field(&stdout, "camera_urls_unique", "true");
    assert_json_field(&stdout, "camera_credentials_redacted", "true");
    assert!(
        stdout.contains(r#""camera_url":"bambu:///local/127.0.0.1?port="#),
        "camera probe did not return the exact loopback scheme: {stdout}"
    );
    for forbidden in [
        "probe-token",
        "studio-camera-a1",
        "studio-camera-a1-mini",
        "studio-camera-p1s",
        "studio-camera-a2l",
        "printer-camera-",
    ] {
        assert!(
            !stdout.contains(forbidden),
            "camera probe output leaked {forbidden}: {stdout}"
        );
    }
}

#[path = "studio_abi_probe/stream_acceptance.rs"]
mod stream_acceptance;
