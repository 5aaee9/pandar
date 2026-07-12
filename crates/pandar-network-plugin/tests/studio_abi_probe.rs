#![cfg(any(unix, windows))]

#[path = "studio_abi_probe/compiler.rs"]
mod compiler;
#[path = "studio_abi_probe/mock_hub.rs"]
mod mock_hub;
#[path = "studio_abi_probe/native_print_error.rs"]
mod native_print_error;
#[path = "studio_abi_probe/run.rs"]
mod run;
mod support;

use mock_hub::MockMode;
use run::{ProbeOutput, run_probe};

fn assert_json_field(output: &str, field: &str, value: &str) {
    assert!(
        output.contains(&format!(r#""{field}":{value}"#)),
        "probe output missing {field}={value}: {output}"
    );
}

fn compiler_identity_is_allowed_for_target(compiler: &str, target_requires_msvc: bool) -> bool {
    !target_requires_msvc || compiler.to_ascii_lowercase().contains("cl.exe")
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
}

#[test]
fn studio_abi_probe_preserves_full_axis_feature_bitmap_and_submits_semantics_through_both_abis() {
    assert!(compiler_identity_is_allowed_for_target("c++", false));
    assert!(!compiler_identity_is_allowed_for_target("c++", true));
    assert!(compiler_identity_is_allowed_for_target("cl.exe", true));
    assert!(mock_hub::required_device_feature_presence_matches(
        r#"{"action":"home","axes":[]}"#,
        false,
    ));
    assert!(!mock_hub::required_device_feature_presence_matches(
        r#"{"action":"home","axes":[],"required_device_features":null}"#,
        false,
    ));
    assert!(mock_hub::required_device_feature_presence_matches(
        r#"{"action":"home","axes":[],"required_device_features":["bambu_mqtt_homing"]}"#,
        true,
    ));

    let ProbeOutput {
        stdout,
        stderr,
        compiler,
    } = run_probe(MockMode::AxisFeatures, "axis-features");

    println!("Studio axis ABI probe compiler: {compiler}");
    #[cfg(all(windows, target_env = "msvc"))]
    assert!(
        compiler_identity_is_allowed_for_target(&compiler, true),
        "axis ABI probe must compile with MSVC cl.exe, got {compiler}"
    );
    assert!(
        stderr.is_empty(),
        "axis probe stderr was not empty: {stderr}"
    );
    let result: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(result["ok"], serde_json::json!(true));
    assert_eq!(result["axis_features_exact"], serde_json::json!(true));
    assert_eq!(result["operation_posts"], serde_json::json!(10));
    let status: serde_json::Value =
        serde_json::from_str(result["cloud_status_body"].as_str().unwrap()).unwrap();
    assert_eq!(
        status["print"]["fun"],
        serde_json::json!("8000004100000020")
    );
}

#[test]
fn probe_refreshes_stale_session_and_preserves_cache_on_invalid_status_refresh() {
    let ProbeOutput { stdout, stderr, .. } =
        run_probe(MockMode::StaleTokenRefresh, "stale-token-refresh");

    assert!(
        stderr.contains("validate Hub printer status refresh response"),
        "probe did not record the invalid status refresh: {stderr}"
    );
    assert!(
        !stderr.contains("token") && !stderr.contains(r#""devices""#),
        "status refresh diagnostic leaked response data: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
    assert_json_field(&stdout, "printer_rc", "0");
    assert_json_field(&stdout, "restored_login", "true");
}

#[test]
fn probe_redacts_failed_hub_responses_through_abi() {
    let ProbeOutput { stdout, stderr, .. } = run_probe(MockMode::Failure, "failure");
    let combined = format!("{stdout}\n{stderr}");

    assert!(
        !combined.contains("secret"),
        "probe leaked secret: {combined}"
    );
    assert!(
        !combined.contains("/tmp/secret.3mf"),
        "probe leaked path: {combined}"
    );
    assert!(combined.contains("invalid_plugin_ticket"));
    assert!(combined.contains("invalid_auth_token"));
    assert!(combined.contains("plugin_forbidden"));
}
