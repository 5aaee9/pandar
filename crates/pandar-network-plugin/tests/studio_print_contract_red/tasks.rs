use std::sync::OnceLock;

use crate::{
    harness::{ProbeEvidence, run_probe},
    pinned::{
        MODEL_TASK_CALLBACK_HASH, MODEL_TASK_FORWARDING_HASH, MODEL_TASK_LAYOUT_HASH,
        MODEL_TASK_STATUS_HASH, SUBTASK_CONSUMER_HASH,
    },
};

#[path = "tasks/artifact.rs"]
mod artifact;
#[path = "tasks/assertions.rs"]
mod assertions;
#[path = "tasks/task_list.rs"]
mod task_list;

#[path = "tasks/trace.rs"]
mod trace;

static TASK_EVIDENCE: OnceLock<ProbeEvidence> = OnceLock::new();

use assertions::assert_model_task_untouched;

fn evidence() -> &'static ProbeEvidence {
    TASK_EVIDENCE.get_or_init(|| run_probe("tasks", "task-contract"))
}

#[test]
fn get_task_plate_index_returns_persisted_plate_for_stable_id() {
    let evidence = evidence();
    assert_eq!(evidence.output["plate_rc"], serde_json::json!(0));
    assert_eq!(evidence.output["plate_index"], serde_json::json!(7));
}

#[test]
fn nonpositive_success_plate_is_remapped_to_bad_gateway_and_cleared() {
    let evidence = run_probe("tasks", "task_invalid_plate_2xx");
    assert_ne!(evidence.output["plate_rc"], serde_json::json!(0));
    assert_eq!(evidence.output["plate_index"], serde_json::json!(-1));
}

#[test]
fn get_subtask_info_matches_pinned_device_manager_double_json_consumer() {
    let evidence = evidence();
    let mut failures = Vec::new();
    if evidence.output["subtask_consumer_hash"] != serde_json::json!(SUBTASK_CONSUMER_HASH) {
        failures.push(
            "compiled subtask consumer is not pinned to DeviceManager.cpp:3886-3985".to_owned(),
        );
    }
    if evidence.output["subtask_rc"] != serde_json::json!(0)
        || evidence.output["subtask_http_code"] != serde_json::json!(200)
    {
        failures.push(format!(
            "subtask result rc={} http={}",
            evidence.output["subtask_rc"], evidence.output["subtask_http_code"]
        ));
    }
    if evidence.output["subtask_consumer_ok"] != serde_json::json!(true) {
        failures.push("pinned DeviceManager consumer rejected subtask JSON".to_owned());
    }
    match evidence.output["subtask_json"]
        .as_str()
        .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok())
    {
        Some(subtask) => {
            let content = subtask["content"]
                .as_str()
                .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok());
            if content.as_ref().map(|value| &value["info"]["plate_idx"])
                != Some(&serde_json::json!(7))
            {
                failures.push("subtask content is not a JSON string with plate_idx=7".to_owned());
            }
            let plate = &subtask["context"]["plates"][0];
            if plate["index"] != serde_json::json!(7)
                || !plate["prediction"].is_i64()
                || !plate["weight"].is_number()
                || !plate["filaments"][0]["used_g"].is_string()
                || !plate["filaments"][0]["used_m"].is_string()
            {
                failures.push(format!("subtask plate field types drifted: {plate}"));
            }
        }
        None => failures.push("get_subtask_info returned no task JSON".to_owned()),
    }
    assert!(
        failures.is_empty(),
        "subtask contract failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn get_slice_info_is_explicitly_unavailable_until_a_pinned_consumer_exists() {
    let evidence = evidence();
    assert_ne!(evidence.output["slice_rc"], serde_json::json!(0));
    assert_eq!(evidence.output["slice_json"], serde_json::json!(""));
}

#[test]
fn model_subtask_populates_the_pinned_caller_owned_object_once() {
    let evidence = evidence();
    for (field, expected) in [
        ("model_task_status_hash", MODEL_TASK_STATUS_HASH),
        ("model_task_layout_hash", MODEL_TASK_LAYOUT_HASH),
        ("model_task_callback_hash", MODEL_TASK_CALLBACK_HASH),
        ("model_task_forwarding_hash", MODEL_TASK_FORWARDING_HASH),
    ] {
        assert_eq!(
            evidence.output[field],
            serde_json::json!(expected),
            "{field}"
        );
    }
    assert_eq!(evidence.output["model_subtask_rc"], serde_json::json!(0));
    assert_ne!(
        evidence.output["model_subtask_null_task_rc"],
        serde_json::json!(0)
    );
    assert_ne!(
        evidence.output["model_subtask_empty_callback_rc"],
        serde_json::json!(0)
    );
    assert_eq!(
        evidence.output["model_subtask_callbacks"],
        serde_json::json!(1)
    );
    assert_eq!(
        evidence.output["model_subtask_same_pointer"],
        serde_json::json!(true)
    );
    for (field, expected) in [
        ("model_subtask_job_id", serde_json::json!(38191)),
        ("model_subtask_design_id", serde_json::json!(0)),
        ("model_subtask_profile_id", serde_json::json!(0)),
        ("model_subtask_instance_id", serde_json::json!(0)),
        ("model_subtask_task_id", serde_json::json!("38191")),
        ("model_subtask_model_id", serde_json::json!("")),
        (
            "model_subtask_model_name",
            serde_json::json!("contract-base-project"),
        ),
        (
            "model_subtask_profile_name",
            serde_json::json!("contract-base-preset"),
        ),
    ] {
        assert_eq!(evidence.output[field], expected, "{field}");
    }
    assert!(evidence.requests.iter().any(|request| {
        request.starts_with("GET /api/v1/plugin/jobs/38191/model-task ")
            && request
                .to_ascii_lowercase()
                .contains("authorization: bearer contract-token")
    }));
}

#[test]
fn model_subtask_failures_never_mutate_or_callback_the_studio_object() {
    for (case, task_id) in [
        ("model_task_metadata_unavailable", "38191"),
        ("model_task_invalid_2xx", "38191"),
        ("stale_model_task", "38191"),
        ("task_unknown", "99999"),
    ] {
        let evidence = run_probe("tasks", case);
        assert_eq!(evidence.output["model_subtask_rc"], serde_json::json!(0));
        assert_model_task_untouched(&evidence, task_id);
    }
}

#[test]
fn destroying_the_agent_joins_an_inflight_model_task_without_callback() {
    let evidence = run_probe("tasks", "model_task_destroy_inflight");
    assert_eq!(evidence.output["model_subtask_rc"], serde_json::json!(0));
    assert_model_task_untouched(&evidence, "38191");
    assert!(
        evidence.output["model_subtask_destroy_ms"]
            .as_i64()
            .unwrap()
            < 2_000,
        "destroy waited {} ms for model-task HTTP",
        evidence.output["model_subtask_destroy_ms"]
    );
    assert!(
        evidence
            .requests
            .iter()
            .any(|request| { request.starts_with("GET /api/v1/plugin/jobs/38191/model-task ") })
    );
}

#[test]
fn destroying_the_agent_cancels_inflight_no_auth_recovery_without_callback() {
    let evidence = run_probe("tasks", "model_task_destroy_no_auth_recovery");
    assert_eq!(evidence.output["model_subtask_rc"], serde_json::json!(0));
    assert_model_task_untouched(&evidence, "38191");
    assert!(
        evidence.output["model_subtask_destroy_ms"]
            .as_i64()
            .unwrap()
            < 2_000,
        "destroy waited {} ms for model-task no-auth recovery",
        evidence.output["model_subtask_destroy_ms"]
    );
    assert!(
        evidence
            .requests
            .iter()
            .any(|request| { request.starts_with("GET /api/v1/plugin/jobs/38191/model-task ") })
    );
    assert!(
        evidence
            .requests
            .iter()
            .any(|request| { request.starts_with("POST /api/v1/plugin/no-auth-session ") })
    );
}

#[test]
fn model_task_callback_serializes_against_account_changes() {
    let evidence = run_probe("tasks", "model_task_callback_account_race");
    assert_eq!(
        evidence.output["model_subtask_callbacks"],
        serde_json::json!(1)
    );
    assert_eq!(
        evidence.output["model_subtask_account_change_returned_during_callback"],
        serde_json::json!(false)
    );
    assert_eq!(
        evidence.output["model_subtask_account_change_rc"],
        serde_json::json!(0)
    );
}

#[test]
fn task_responses_are_discarded_when_the_account_changes_during_http() {
    for (case, result_field, body_field) in [
        ("stale_task_list", "tasks_rc", "tasks_body"),
        ("stale_task_plate", "plate_rc", "tasks_body"),
        ("stale_task_subtask", "subtask_rc", "subtask_json"),
    ] {
        let evidence = run_probe("tasks", case);
        assert_ne!(
            evidence.output[result_field],
            serde_json::json!(0),
            "{case}"
        );
        let body = evidence.output[body_field].as_str().unwrap_or_default();
        if case != "stale_task_plate" {
            assert!(
                !body.contains("contract-base.3mf"),
                "{case}: stale body leaked"
            );
        }
        if case == "stale_task_plate" {
            assert_eq!(evidence.output["plate_index"], serde_json::json!(-1));
        }
        if case == "stale_task_subtask" {
            assert_eq!(evidence.output["subtask_http_code"], serde_json::json!(409));
        }
    }
}

#[test]
fn unknown_plate_and_subtask_are_explicitly_unavailable() {
    let evidence = run_probe("tasks", "task_unknown");
    assert_ne!(evidence.output["plate_rc"], serde_json::json!(0));
    assert_eq!(evidence.output["plate_index"], serde_json::json!(-1));
    assert_ne!(evidence.output["subtask_rc"], serde_json::json!(0));
    assert_eq!(evidence.output["subtask_http_code"], serde_json::json!(404));
    assert_eq!(evidence.output["subtask_json"], serde_json::json!(""));
    assert!(
        evidence.output["subtask_http_body"]
            .as_str()
            .unwrap()
            .contains("job_not_found")
    );
}

#[test]
fn unusable_subtask_metadata_preserves_hub_conflict_and_clears_json() {
    let evidence = run_probe("tasks", "task_metadata_unavailable");
    assert_ne!(evidence.output["subtask_rc"], serde_json::json!(0));
    assert_eq!(evidence.output["subtask_http_code"], serde_json::json!(409));
    assert_eq!(evidence.output["subtask_json"], serde_json::json!(""));
    assert!(
        evidence.output["subtask_http_body"]
            .as_str()
            .unwrap()
            .contains("studio_task_metadata_unavailable")
    );
}

#[test]
fn invalid_success_subtasks_are_remapped_to_bad_gateway_and_cleared() {
    for case in [
        "task_invalid_subtask_2xx",
        "task_oversized_subtask_weight_2xx",
        "task_oversized_subtask_prediction_2xx",
        "task_nonpositive_subtask_plate_2xx",
        "task_mixed_invalid_subtask_2xx",
    ] {
        let evidence = run_probe("tasks", case);
        assert_ne!(
            evidence.output["subtask_rc"],
            serde_json::json!(0),
            "{case}"
        );
        assert_eq!(
            evidence.output["subtask_http_code"],
            serde_json::json!(502),
            "{case}"
        );
        assert_eq!(
            evidence.output["subtask_json"],
            serde_json::json!(""),
            "{case}"
        );
        assert!(
            evidence.output["subtask_http_body"]
                .as_str()
                .unwrap()
                .contains("invalid_response"),
            "{case}"
        );
    }
}

#[test]
fn malformed_success_task_pages_are_remapped_to_bad_gateway() {
    for case in [
        "task_oversized_total_2xx",
        "task_ambiguous_title_2xx",
        "task_nonempty_cover_2xx",
    ] {
        let evidence = run_probe("tasks", case);
        assert_ne!(evidence.output["tasks_rc"], serde_json::json!(0), "{case}");
        assert!(
            evidence.output["tasks_body"]
                .as_str()
                .unwrap()
                .contains("invalid_response"),
            "{case}"
        );
    }
}

#[test]
fn external_task_json_diagnostics_never_echo_sensitive_values() {
    for case in ["task_sensitive_page_2xx", "task_sensitive_error_4xx"] {
        let evidence = run_probe("tasks", case);
        assert_ne!(evidence.output["tasks_rc"], serde_json::json!(0), "{case}");
        evidence.assert_excludes(crate::harness::DIAGNOSTIC_SECRET);
    }
}

#[test]
fn task_list_surfaces_hub_outage_without_cached_data() {
    let evidence = run_probe("tasks", "task_hub_outage");
    assert_ne!(evidence.output["tasks_rc"], serde_json::json!(0));
    assert!(
        evidence.output["tasks_body"]
            .as_str()
            .unwrap()
            .contains("hub_unavailable")
    );
    assert!(
        !evidence.output["tasks_body"]
            .as_str()
            .unwrap()
            .contains("contract-base.3mf")
    );
}
