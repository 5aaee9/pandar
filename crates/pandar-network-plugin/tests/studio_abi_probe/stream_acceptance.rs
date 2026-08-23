use super::*;

#[test]
fn probe_server_connectivity_follows_readyz_transitions() {
    let ProbeOutput { stdout, stderr, .. } =
        run_probe(MockMode::ConnectionReadiness, "connection-readiness");

    assert!(
        stderr.is_empty(),
        "connection readiness probe wrote stderr: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
}

#[test]
fn probe_background_printer_timeout_invalidates_connectivity_without_stale_status() {
    let ProbeOutput { stdout, stderr, .. } =
        run_probe(MockMode::BackgroundTimeout, "background-timeout");

    // The transient close must not surface a disconnect; only the bounded
    // healthz observation during the outage may report one.
    assert!(
        !stderr.is_empty(),
        "background timeout probe did not record the stream outage: {stderr}"
    );
    assert!(
        !stderr.contains("/api/v1/plugin/printers"),
        "outage probe polled the retired printer list: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
}

#[test]
fn probe_healthy_hub_stream_outage_reports_one_stale_error_and_recovers() {
    let ProbeOutput { stdout, stderr, .. } =
        run_probe(MockMode::StreamUnavailable, "stream-unavailable");

    assert!(
        !stderr.contains("/api/v1/plugin/printers"),
        "stream-unavailable probe used the retired list endpoint: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
}

#[test]
fn probe_authenticated_rejection_preserves_reachability_and_reports_code_five_once() {
    let ProbeOutput {
        stdout,
        stderr: _stderr,
        ..
    } = run_probe(MockMode::AuthRejected, "auth-rejected");

    assert_json_field(&stdout, "ok", "true");
}

#[test]
fn probe_printer_presence_requires_fresh_typed_online_observations() {
    let ProbeOutput { stdout, stderr, .. } =
        run_probe(MockMode::PrinterPresence, "printer-presence");

    assert!(
        !stderr.contains("/api/v1/plugin/printers") && !stderr.contains("connect callback=-2"),
        "printer presence probe used polling or disconnected on a short flap: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
}

#[test]
fn probe_same_token_account_transition_fences_reentrant_printer_state() {
    let ProbeOutput { stdout, stderr, .. } =
        run_probe(MockMode::AccountTransition, "account-transition");

    assert!(
        stderr.is_empty(),
        "account transition probe stderr was not empty: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
}

#[test]
fn probe_no_auth_startup_recovers_once_after_hub_becomes_available() {
    let ProbeOutput { stdout, stderr, .. } =
        run_probe(MockMode::NoAuthRecovery, "no-auth-recovery");
    let stderr_lower = stderr.to_ascii_lowercase();

    assert!(
        stderr_lower.contains("connection refused")
            || stderr_lower.contains("actively refused")
            || stderr_lower.contains("os error 10061"),
        "no-auth recovery probe missed the initial connect failure: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
}

#[test]
fn probe_official_studio_lifecycle_recovers_without_foreground_abi_polling() {
    let ProbeOutput { stdout, stderr, .. } = run_probe(
        MockMode::OfficialNoAuthRecovery,
        "official-no-auth-recovery",
    );
    let stderr_lower = stderr.to_ascii_lowercase();

    assert!(
        stderr_lower.contains("connection refused")
            || stderr_lower.contains("actively refused")
            || stderr_lower.contains("os error 10061"),
        "official lifecycle probe missed the initial connect failure: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
}

#[test]
fn probe_logged_out_notification_preserves_official_no_auth_recovery() {
    let ProbeOutput { stdout, stderr, .. } = run_probe(
        MockMode::OfficialNoAuthLogoutRecovery,
        "official-no-auth-logout-recovery",
    );
    let stderr_lower = stderr.to_ascii_lowercase();

    assert!(
        stderr_lower.contains("connection refused")
            || stderr_lower.contains("actively refused")
            || stderr_lower.contains("os error 10061"),
        "logout recovery probe missed the initial connect failure: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
}

#[test]
fn probe_serializes_ticket_exchange_with_later_account_mutation() {
    let ProbeOutput { stdout, stderr, .. } =
        run_probe(MockMode::AccountExchangeRace, "account-exchange-race");

    assert!(
        stderr.contains(
            "pandar ticket candidate revoke failed: status=1 http_code=503 body={\"error\":\"invalid_response\"}"
        ) && !stderr.contains("fifo-login-token"),
        "account exchange race missed the redacted candidate revoke failure: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
}

#[test]
fn probe_online_callbacks_recheck_freshness_at_final_claim() {
    let ProbeOutput { stdout, stderr, .. } = run_probe(MockMode::FreshnessClaim, "freshness-claim");

    assert!(
        stderr.is_empty(),
        "freshness claim probe stderr was not empty: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
}

#[test]
fn probe_callback_gate_orders_offline_and_recovery_at_final_claim() {
    let ProbeOutput { stdout, stderr, .. } = run_probe(MockMode::CallbackOrder, "callback-order");

    assert!(
        stderr.is_empty(),
        "callback order probe stderr was not empty: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
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
    assert_eq!(
        status["print"]["cfg"],
        serde_json::json!("8000000000000001")
    );
    assert_eq!(status["print"]["aux"], serde_json::json!("A4003001"));
    assert_eq!(status["print"]["stat"], serde_json::json!("1000000001"));
    assert_ne!(status["print"]["wifi_signal"], serde_json::json!("100%"));
    assert_eq!(status["print"]["sdcard"], serde_json::json!(false));
    assert_eq!(status["print"]["ipcam"]["ipcam_dev"], "0");
    assert_eq!(status["print"]["ipcam"]["liveview"]["local"], "none");
    assert_eq!(status["print"]["ipcam"]["liveview"]["remote"], "none");
    assert_eq!(status["print"]["ipcam"]["rtsp_url"], "");
    assert_eq!(
        status["print"]["support_mqtt_alive"],
        serde_json::json!(true)
    );
    assert_eq!(status["print"]["support_chamber"], serde_json::json!(false));
    assert_eq!(
        status["print"]["support_chamber_temp_display"],
        serde_json::json!(false)
    );
    assert_eq!(
        status["print"]["ams"]["ams"][0]["info"],
        serde_json::json!("00000E00")
    );
    assert_eq!(
        status["print"]["ams"]["ams"][1]["info"],
        serde_json::json!("01000E00")
    );
}

#[test]
fn probe_successful_token_rotation_preserves_state_and_invalid_status_suppresses_stale_output() {
    let ProbeOutput { stdout, stderr, .. } = run_probe(MockMode::TokenRotation, "token-rotation");

    assert!(
        !stderr.contains("/api/v1/plugin/printers") && !stderr.contains(r#""devices""#),
        "stream rotation used polling or leaked response data: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
    assert_json_field(&stdout, "printer_rc", "0");
}

#[test]
fn probe_token_rotation_retry_offline_disconnects_cloud_and_local_once() {
    let ProbeOutput { stdout, stderr, .. } =
        run_probe(MockMode::TokenRotationOffline, "token-rotation-offline");

    assert!(
        !stderr.contains("/api/v1/plugin/printers")
            && !stderr.contains(r#""devices""#)
            && !stderr.contains("secret"),
        "offline rotation leaked data or used polling: {stderr}"
    );
    assert_json_field(&stdout, "ok", "true");
    assert_json_field(&stdout, "printer_rc", "0");
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
    assert!(combined.contains("invalid_plugin_ticket"), "{combined}");
    assert!(
        combined.contains("invalid_auth_token") || combined.contains("cache_initializing"),
        "{combined}"
    );
    assert!(combined.contains("plugin_forbidden"), "{combined}");
}
