use crate::pinned::TASK_CONSUMER_HASH;

use super::{artifact::assert_plugin_identity_reported, evidence};

#[test]
fn get_user_tasks_matches_pinned_task_manager_consumer_shape() {
    let evidence = evidence();
    assert_plugin_identity_reported(evidence);
    let mut failures = Vec::new();
    if evidence.output["task_consumer_hash"] != serde_json::json!(TASK_CONSUMER_HASH) {
        failures.push("compiled task consumer is not pinned to TaskManager.cpp:321-381".to_owned());
    }
    if evidence.output["task_consumer_ok"] != serde_json::json!(true) {
        failures.push("pinned task consumer rejected get_user_tasks body".to_owned());
    }
    if evidence.output["tasks_rc"] != serde_json::json!(0) {
        failures.push(format!("get_user_tasks rc={}", evidence.output["tasks_rc"]));
    }
    let tasks = evidence.output["tasks_body"]
        .as_str()
        .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok());
    let Some(tasks) = tasks else {
        panic!(
            "get_user_tasks did not return JSON: {}",
            evidence.output["tasks_body"]
        );
    };
    if tasks["total"] != serde_json::json!(1) || tasks["hits"].as_array().map(Vec::len) != Some(1) {
        failures.push(format!("expected total=1 and one hit, got {tasks}"));
    } else {
        let hit = &tasks["hits"][0];
        for (field, expected) in [
            ("id", serde_json::json!(38191)),
            ("status", serde_json::json!(1)),
            ("designId", serde_json::json!(0)),
            ("profileId", serde_json::json!(38191)),
            ("deviceId", serde_json::json!("studio-serial-1")),
        ] {
            if hit[field] != expected {
                failures.push(format!(
                    "task hit {field} expected {expected}, got {}",
                    hit[field]
                ));
            }
        }
    }
    if !evidence.requests.iter().any(|request| {
        request.starts_with("GET /api/v1/plugin/jobs?")
            && request.contains("dev_id=studio-serial-1")
            && request.contains("status=1")
            && request.contains("offset=0")
            && request.contains("limit=5")
    }) {
        failures.push("get_user_tasks did not server-filter dev/status/offset/limit".to_owned());
    }
    assert!(
        failures.is_empty(),
        "task-list contract failures:\n{}",
        failures.join("\n")
    );
}
