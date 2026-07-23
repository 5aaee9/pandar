use crate::harness::ProbeEvidence;

pub(super) fn assert_model_task_untouched(evidence: &ProbeEvidence, task_id: &str) {
    for event in [
        "model-task callback started",
        "model-task callback returned",
    ] {
        assert!(
            !evidence.trace.lines().any(|line| line == event),
            "failed model task emitted {event}"
        );
    }
    assert_eq!(
        evidence.output["model_subtask_callbacks"],
        serde_json::json!(0)
    );
    assert_eq!(
        evidence.output["model_subtask_same_pointer"],
        serde_json::json!(false)
    );
    for (field, expected) in [
        ("model_subtask_job_id", serde_json::json!(-701)),
        ("model_subtask_design_id", serde_json::json!(-702)),
        ("model_subtask_profile_id", serde_json::json!(-703)),
        ("model_subtask_instance_id", serde_json::json!(-704)),
        ("model_subtask_task_id", serde_json::json!(task_id)),
        ("model_subtask_model_id", serde_json::json!("model-before")),
        ("model_subtask_model_name", serde_json::json!("name-before")),
        (
            "model_subtask_profile_name",
            serde_json::json!("profile-before"),
        ),
    ] {
        assert_eq!(evidence.output[field], expected, "{field}");
    }
}
