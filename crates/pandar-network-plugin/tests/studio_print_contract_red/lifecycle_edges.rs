use crate::harness::run_probe;

fn stages(case: &str) -> serde_json::Value {
    run_probe("print", case).output["stages"].clone()
}

#[test]
fn upload_chunk_cancellation_returns_minus_18_before_durable_create() {
    let evidence = run_probe("print", "cancel_upload");
    assert_eq!(evidence.output["rc"], serde_json::json!(-18));
    assert_eq!(evidence.output["stages"][0], serde_json::json!(0));
    assert!(
        evidence.output["stages"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(1))
    );
    assert_eq!(
        evidence.output["stages"].as_array().unwrap().last(),
        Some(&serde_json::json!(7))
    );
    assert_eq!(
        evidence.output["codes"].as_array().unwrap().last(),
        Some(&serde_json::json!(-18))
    );
}

#[test]
fn cancellation_after_admission_is_repolled_before_create() {
    let evidence = run_probe("print", "cancel_before_create");
    assert_eq!(evidence.output["rc"], serde_json::json!(-18));
    assert_eq!(evidence.output["stages"], serde_json::json!([7]));
    assert_eq!(evidence.output["codes"], serde_json::json!([-18]));
    assert!(
        !evidence
            .requests
            .iter()
            .any(|request| request.starts_with("POST /api/v1/plugin/prints "))
    );
}

#[test]
fn queued_cancellation_requires_hub_confirmation_before_minus_18() {
    let evidence = run_probe("print", "cancel_queued");
    assert_eq!(evidence.output["rc"], serde_json::json!(-18));
    assert_eq!(evidence.output["stages"], serde_json::json!([0, 1, 2, 7]));
    assert!(
        evidence
            .requests
            .iter()
            .any(|request| request.starts_with("POST /api/v1/plugin/jobs/38191/cancel "))
    );
}

#[test]
fn mismatched_cancel_confirmation_never_returns_minus_18() {
    let evidence = run_probe("print", "cancel_wrong_id");
    assert_eq!(evidence.output["rc"], serde_json::json!(-19));
    assert_eq!(evidence.output["stages"], serde_json::json!([0, 1, 2, 7]));
    assert_eq!(
        evidence.output["bodies"].as_array().unwrap().last(),
        Some(&serde_json::json!(r#"{"error":"invalid_response"}"#))
    );
}

#[test]
fn acknowledged_cancel_too_late_is_not_reported_as_cancelled() {
    let evidence = run_probe("print", "cancel_too_late");
    assert_eq!(evidence.output["rc"], serde_json::json!(-19));
    assert_eq!(
        evidence.output["stages"],
        serde_json::json!([0, 1, 2, 3, 7])
    );
    assert_eq!(
        evidence.output["bodies"].as_array().unwrap().last(),
        Some(&serde_json::json!(r#"{"error":"cancel_too_late"}"#))
    );
}

#[test]
fn downstream_agent_failure_never_emits_finished() {
    let evidence = run_probe("print", "downstream_failure");
    assert_eq!(evidence.output["rc"], serde_json::json!(-19));
    assert_eq!(evidence.output["stages"], serde_json::json!([0, 1, 2, 7]));
    assert!(
        !evidence.output["stages"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(6))
    );
}

#[test]
fn physical_abort_after_publish_is_not_submission_cancellation() {
    let evidence = run_probe("print", "physical_abort_after_publish");
    assert_eq!(evidence.output["rc"], serde_json::json!(0));
    assert_eq!(
        evidence.output["stages"],
        serde_json::json!([0, 1, 2, 3, 4, 5, 6])
    );
    assert_eq!(
        evidence.output["bodies"].as_array().unwrap().last(),
        Some(&serde_json::json!("3"))
    );
}

#[test]
fn false_wait_result_prevents_finished() {
    assert_eq!(
        stages("wait_false"),
        serde_json::json!([0, 1, 2, 3, 4, 5, 7])
    );
}

#[test]
fn cancellation_after_wait_requires_confirmation_and_prevents_finished() {
    let evidence = run_probe("print", "cancel_after_wait");
    assert_eq!(evidence.output["rc"], serde_json::json!(-19));
    assert_eq!(
        evidence.output["stages"],
        serde_json::json!([0, 1, 2, 3, 4, 5, 7])
    );
    assert_eq!(evidence.output["wait_count"], serde_json::json!(1));
    assert_eq!(
        evidence.output["bodies"].as_array().unwrap().last(),
        Some(&serde_json::json!(r#"{"error":"cancel_too_late"}"#))
    );
}

#[test]
fn stage_five_cancellation_is_confirmed_before_wait() {
    let evidence = run_probe("print", "cancel_at_stage_five");
    assert_eq!(evidence.output["rc"], serde_json::json!(-18));
    assert_eq!(
        evidence.output["stages"],
        serde_json::json!([0, 1, 2, 3, 4, 5, 7])
    );
    assert_eq!(evidence.output["wait_count"], serde_json::json!(0));
}

#[test]
fn cancellation_after_false_wait_is_confirmed_before_wait_failure() {
    let evidence = run_probe("print", "cancel_during_failed_wait");
    assert_eq!(evidence.output["rc"], serde_json::json!(-18));
    assert_eq!(
        evidence.output["stages"],
        serde_json::json!([0, 1, 2, 3, 4, 5, 7])
    );
    assert_eq!(evidence.output["wait_count"], serde_json::json!(1));
}

#[test]
fn account_change_during_job_detail_discards_the_response() {
    let evidence = run_probe("print", "stale_during_detail");
    assert_ne!(evidence.output["rc"], serde_json::json!(0));
    assert_eq!(evidence.output["stages"], serde_json::json!([0, 1, 2, 7]));
}

#[test]
fn freshness_change_wins_over_cancel_when_confirmation_fails() {
    let evidence = run_probe("print", "stale_cancel_failed");
    assert_eq!(evidence.output["rc"], serde_json::json!(0));
    assert_eq!(evidence.output["stages"], serde_json::json!([0, 1, 2, 3]));
    assert!(evidence.stderr.contains("unconfirmed stale cancellation"));
    assert_eq!(
        evidence
            .requests
            .iter()
            .filter(|request| request.starts_with("POST /api/v1/plugin/jobs/38191/cancel "))
            .count(),
        1
    );
}

#[test]
fn freshness_change_during_cancel_failure_retains_the_accepted_job() {
    let evidence = run_probe("print", "cancel_race_stale");
    assert_eq!(evidence.output["rc"], serde_json::json!(0));
    assert_eq!(evidence.output["stages"], serde_json::json!([0, 1, 2]));
    assert!(evidence.stderr.contains("unconfirmed stale cancellation"));
    assert_eq!(
        evidence
            .requests
            .iter()
            .filter(|request| request.starts_with("POST /api/v1/plugin/jobs/38191/cancel "))
            .count(),
        1
    );
}

#[test]
fn lifecycle_hub_outage_is_reported_without_finished() {
    let evidence = run_probe("print", "lifecycle_hub_outage");
    assert_eq!(evidence.output["rc"], serde_json::json!(-19));
    assert!(
        !evidence.output["stages"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(6))
    );
    assert_eq!(
        evidence.output["bodies"].as_array().unwrap().last(),
        Some(&serde_json::json!(r#"{"error":"hub_unavailable"}"#))
    );
}

#[test]
fn external_lifecycle_json_diagnostics_never_echo_sensitive_values() {
    for case in [
        "lifecycle_sensitive_page_2xx",
        "lifecycle_sensitive_error_4xx",
    ] {
        let evidence = run_probe("print", case);
        assert_eq!(evidence.output["rc"], serde_json::json!(-19), "{case}");
        evidence.assert_excludes(crate::harness::DIAGNOSTIC_SECRET);
    }
}
