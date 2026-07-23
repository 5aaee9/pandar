use crate::harness::{print_requests, run_probe};

#[test]
fn studio_cancellation_returns_minus_18_without_creating_a_job() {
    let evidence = run_probe("print", "cancel");
    assert_eq!(evidence.output["rc"], serde_json::json!(-18));
    assert!(print_requests(&evidence).is_empty());
    assert_eq!(evidence.output["wait_count"], serde_json::json!(0));
}

#[test]
fn hub_201_does_not_skip_delivery_stages_or_wait_contract() {
    let evidence = run_probe("print", "lifecycle");
    let stages = evidence.output["stages"].as_array().unwrap();
    assert!(stages.len() >= 7, "missing lifecycle stages: {stages:?}");
    assert_eq!(stages.first(), Some(&serde_json::json!(0)));
    assert!(stages[1..stages.len() - 5].iter().all(|stage| *stage == 1));
    assert_eq!(
        &stages[stages.len() - 5..],
        &[
            serde_json::json!(2),
            serde_json::json!(3),
            serde_json::json!(4),
            serde_json::json!(5),
            serde_json::json!(6),
        ]
    );
    assert_eq!(evidence.output["wait_count"], serde_json::json!(1));
    assert_eq!(evidence.output["wait_state"], serde_json::json!(0));
    let wait_info: serde_json::Value =
        serde_json::from_str(evidence.output["wait_info"].as_str().unwrap()).unwrap();
    assert_eq!(wait_info, serde_json::json!({"job_id": 38191}));
    assert_eq!(evidence.output["rc"], serde_json::json!(0));
}

#[test]
fn unconfirmed_stale_snapshot_cancel_remains_accepted_and_visible() {
    let evidence = run_probe("print", "stale_after_201");
    assert_eq!(evidence.output["rc"], serde_json::json!(0));
    assert_eq!(evidence.output["stages"], serde_json::json!([0, 1, 2]));
    assert_eq!(evidence.output["wait_count"], serde_json::json!(0));
    assert!(evidence.stderr.contains("unconfirmed stale cancellation"));
    assert!(!evidence.stderr.contains("contract-token"));
    assert!(!evidence.stderr.contains(&evidence.artifact_path));
    assert!(
        evidence
            .requests
            .iter()
            .any(|request| request.starts_with("POST /api/v1/plugin/jobs/38191/cancel "))
    );
    assert!(
        !evidence
            .requests
            .iter()
            .any(|request| request.starts_with("GET /api/v1/plugin/jobs/38191 "))
    );
}

#[test]
fn normalized_trailing_slash_hub_url_survives_local_webserver_startup() {
    let evidence = run_probe("print", "trailing_slash_hub");
    assert_eq!(evidence.output["rc"], serde_json::json!(0));
    assert_eq!(
        evidence.output["stages"],
        serde_json::json!([0, 1, 2, 3, 4, 5, 6])
    );
    assert_eq!(print_requests(&evidence).len(), 1);
}
