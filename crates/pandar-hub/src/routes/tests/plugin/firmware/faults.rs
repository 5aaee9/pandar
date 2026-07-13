use axum::http::{Method, StatusCode};
use pandar_core::CommandId;
use serde_json::json;

use super::super::*;
use super::support::*;

#[tokio::test]
async fn firmware_prepare_persistence_faults_before_and_after_dispatch_are_explicit_pre_publish() {
    let mut before = FirmwareRouteFixture::new("firmware-prepare-persist-before").await;
    before
        .execute_sqlite(
            "CREATE TRIGGER task6_fail_prepare_insert BEFORE INSERT ON commands WHEN NEW.kind = 'firmware_control' BEGIN SELECT RAISE(ABORT, 'TASK6-PREPARE-INSERT-CAUSE'); END",
        )
        .await;
    let (status, body) = request_as(
        before.app(),
        Method::POST,
        &before.uri("/prepare"),
        Some(upgrade_metadata("persist-before")),
        &before.auth,
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["phase"], "pre_publish_failure");
    assert_eq!(body["error"], "internal_server_error");
    assert!(!body.to_string().contains("TASK6-PREPARE-INSERT-CAUSE"));
    assert!(before.commands.try_recv().is_err());

    let mut after = FirmwareRouteFixture::new("firmware-prepare-persist-after").await;
    let request = after.spawn_json(Method::POST, "/prepare", upgrade_metadata("persist-after"));
    let outbound = after.next_command().await;
    after
        .execute_sqlite(
            "CREATE TRIGGER task6_fail_prepare_terminal BEFORE UPDATE OF status ON commands WHEN NEW.status = 'failed' BEGIN SELECT RAISE(ABORT, 'TASK6-PREPARE-TERMINAL-CAUSE'); END",
        )
        .await;
    after
        .generic_failure(
            command_id(&outbound),
            "prepare dispatch failed with secret cause",
        )
        .await;
    let (status, body) = request.await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["phase"], "pre_publish_failure");
    let text = body.to_string();
    assert!(!text.contains("secret cause"));
    assert!(!text.contains("TASK6-PREPARE-TERMINAL-CAUSE"));
}

#[tokio::test]
async fn firmware_execute_fault_before_dispatch_is_safe_but_after_dispatch_persistence_is_unknown()
{
    let mut before = FirmwareRouteFixture::new("firmware-execute-before-dispatch").await;
    let (prepared, _) = before.prepare(upgrade_metadata("before-dispatch")).await;
    before
        .execute_sqlite(
            "CREATE TRIGGER task6_fail_execute_sent BEFORE UPDATE OF status ON commands WHEN NEW.status = 'acknowledged' BEGIN SELECT RAISE(ABORT, 'TASK6-EXECUTE-SENT-CAUSE'); END",
        )
        .await;
    let (status, body) = request_as(
        before.app(),
        Method::POST,
        &before.uri("/execute"),
        Some(json!({
            "prepared_token": prepared["prepared_token"],
            "command": upgrade_command("before-dispatch")
        })),
        &before.auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["phase"], "pre_publish_failure");
    assert!(before.commands.try_recv().is_err());
    assert!(!body.to_string().contains("TASK6-EXECUTE-SENT-CAUSE"));

    let mut after = FirmwareRouteFixture::new("firmware-execute-after-dispatch").await;
    let (prepared, _) = after
        .prepare(start_metadata("after-dispatch", "ota", "v"))
        .await;
    let command_id = CommandId::parse(prepared["command_id"].as_str().unwrap()).unwrap();
    let request = after.spawn_json(
        Method::POST,
        "/execute",
        json!({
            "prepared_token": prepared["prepared_token"],
            "command": start_command("after-dispatch", URL_SENTINEL, "ota", "v")
        }),
    );
    let _ = after.next_command().await;
    after
        .execute_sqlite(
            "CREATE TRIGGER task6_fail_execute_terminal BEFORE UPDATE OF status ON commands WHEN NEW.status = 'failed' BEGIN SELECT RAISE(ABORT, 'TASK6-EXECUTE-TERMINAL-CAUSE'); END",
        )
        .await;
    after
        .generic_failure(
            command_id,
            &format!("dispatch failed after execute {URL_SENTINEL}"),
        )
        .await;
    let (status, body) = request.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["phase"], "outcome_unknown");
    let text = body.to_string();
    assert!(!text.contains(URL_SENTINEL));
    assert!(!text.contains("user:secret"));
    assert!(!text.contains("TASK6-EXECUTE-TERMINAL-CAUSE"));
}

#[tokio::test]
async fn firmware_execute_internal_repository_failure_before_dispatch_is_explicit_pre_publish() {
    let mut fixture = FirmwareRouteFixture::new("firmware-execute-pre-dispatch-internal").await;
    let (prepared, _) = fixture
        .prepare(upgrade_metadata("pre-dispatch-internal"))
        .await;
    fixture
        .execute_sqlite(
            "UPDATE printers SET firmware_modules_json = '{malformed-task6-json' WHERE id IS NOT NULL",
        )
        .await;

    let (status, body) = request_as(
        fixture.app(),
        Method::POST,
        &fixture.uri("/execute"),
        Some(json!({
            "prepared_token": prepared["prepared_token"],
            "command": upgrade_command("pre-dispatch-internal")
        })),
        &fixture.auth,
    )
    .await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"], "internal_server_error");
    assert_eq!(body["phase"], "pre_publish_failure");
    assert!(!body.to_string().contains("malformed-task6-json"));
    assert!(fixture.commands.try_recv().is_err());
}

#[tokio::test]
async fn firmware_execute_fault_after_publish_and_during_terminal_persistence_remains_unknown() {
    let mut published = FirmwareRouteFixture::new("firmware-execute-after-publish").await;
    let (prepared, _) = published
        .prepare(start_metadata("published", "ota", "v"))
        .await;
    let command_id = CommandId::parse(prepared["command_id"].as_str().unwrap()).unwrap();
    let request = published.spawn_json(
        Method::POST,
        "/execute",
        json!({
            "prepared_token": prepared["prepared_token"],
            "command": start_command("published", URL_SENTINEL, "ota", "v")
        }),
    );
    let _ = published.next_command().await;
    published.published(command_id).await;
    published
        .generic_failure(
            command_id,
            &format!("acknowledgement transport failed {URL_SENTINEL}"),
        )
        .await;
    let (status, body) = request.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["phase"], "outcome_unknown");
    assert!(!body.to_string().contains(URL_SENTINEL));
    assert!(!body.to_string().contains("user:secret"));

    let mut terminal = FirmwareRouteFixture::new("firmware-execute-terminal-persistence").await;
    let (prepared, _) = terminal
        .prepare(start_metadata("terminal", "ota", "v"))
        .await;
    let command_id = CommandId::parse(prepared["command_id"].as_str().unwrap()).unwrap();
    let request = terminal.spawn_json(
        Method::POST,
        "/execute",
        json!({
            "prepared_token": prepared["prepared_token"],
            "command": start_command("terminal", URL_SENTINEL, "ota", "v")
        }),
    );
    let _ = terminal.next_command().await;
    terminal
        .execute_sqlite(
            "CREATE TRIGGER task6_fail_terminal_success BEFORE UPDATE OF status ON commands WHEN NEW.status = 'succeeded' BEGIN SELECT RAISE(ABORT, 'TASK6-SUCCESS-TERMINAL-CAUSE'); END",
        )
        .await;
    assert!(
        terminal
            .acknowledgement_result(command_id, "terminal")
            .await
            .is_err()
    );
    let (status, body) = request.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["phase"], "outcome_unknown");
    assert!(body.get("outcome").is_none());
    assert!(!body.to_string().contains("TASK6-SUCCESS-TERMINAL-CAUSE"));
    assert!(!body.to_string().contains(URL_SENTINEL));
}

impl FirmwareRouteFixture {
    async fn execute_sqlite(&self, sql: &'static str) {
        sqlx::query(sql)
            .execute(sqlite_pool(&self.state))
            .await
            .unwrap();
    }
}
