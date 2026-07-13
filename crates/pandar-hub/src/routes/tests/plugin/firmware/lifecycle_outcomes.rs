use axum::http::{Method, StatusCode};
use pandar_core::CommandId;
use serde_json::json;

use super::support::*;
use crate::protocol::agent::v1::{
    FirmwareAcknowledgement, PrinterFirmwareStatus, PrinterUpgradeState,
    PublishedWithoutAcknowledgement, firmware_command_result,
};

#[tokio::test]
async fn firmware_generation_replacement_during_refresh_prepare_execute_and_publish_is_classified()
{
    let mut refresh = FirmwareRouteFixture::new("firmware-generation-refresh").await;
    let request = refresh.spawn_json(
        Method::POST,
        "/refresh",
        json!({"sequence_id":"generation-refresh"}),
    );
    let _ = refresh.next_command().await;
    refresh.invalidated(GENERATION + 1).await;
    let (status, body) = request.await.unwrap();
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(body["error"], "firmware_refresh_failed");

    let mut prepare = FirmwareRouteFixture::new("firmware-generation-prepare").await;
    let request = prepare.spawn_json(Method::POST, "/prepare", upgrade_metadata("prepare"));
    let _ = prepare.next_command().await;
    prepare.invalidated(GENERATION + 1).await;
    let (status, body) = request.await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["phase"], "pre_publish_failure");

    assert_generation_execute_phase(false, "firmware-generation-execute").await;
    assert_generation_execute_phase(true, "firmware-generation-published").await;
}

#[tokio::test]
async fn firmware_execute_http_serializes_rejection_transient_status_and_published_without_ack() {
    let mut rejected = FirmwareRouteFixture::new("firmware-execute-http-rejected").await;
    let (prepared, _) = rejected.prepare(upgrade_metadata("rejected")).await;
    let command_id = CommandId::parse(prepared["command_id"].as_str().unwrap()).unwrap();
    let request = rejected.spawn_json(
        Method::POST,
        "/execute",
        json!({
            "prepared_token": prepared["prepared_token"],
            "command": upgrade_command("rejected")
        }),
    );
    let _ = rejected.next_command().await;
    rejected
        .typed_result(
            command_id,
            Some(PrinterFirmwareStatus {
                upgrade_state: Some(PrinterUpgradeState {
                    status: Some("SWITCHING".to_owned()),
                    progress: Some("0".to_owned()),
                    message: Some(String::new()),
                    module: None,
                    error_code: Some(0),
                    new_version_state: None,
                    consistency_request: Some(false),
                    force_upgrade: Some(false),
                    display_state: None,
                    ota_new_version_number: None,
                    ams_new_version_number: None,
                    ahb_new_version_number: None,
                    new_versions: None,
                    ams_firmware: None,
                }),
                cfg: Some(String::new()),
            }),
            firmware_command_result::Outcome::Acknowledgement(FirmwareAcknowledgement {
                command: "upgrade_confirm".to_owned(),
                sequence_id: "rejected".to_owned(),
                result: Some("fail".to_owned()),
                error_code: Some(17),
                reason: Some("printer rejected".to_owned()),
                message: Some("not compatible".to_owned()),
            }),
        )
        .await
        .unwrap();
    let (status, body) = request.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["phase"], "rejected");
    assert_eq!(body["outcome"]["outcome"], "acknowledged");
    assert_eq!(body["outcome"]["acknowledgement"]["result"], "fail");
    assert_eq!(body["outcome"]["acknowledgement"]["err_code"], 17);
    assert_eq!(
        body["outcome"]["acknowledgement"]["reason"],
        "printer rejected"
    );
    assert_eq!(
        body["transient_status"]["upgrade_state"]["status"],
        "SWITCHING"
    );
    assert_eq!(body["transient_status"]["upgrade_state"]["progress"], "0");
    assert_eq!(body["transient_status"]["upgrade_state"]["err_code"], 0);
    assert_eq!(
        body["transient_status"]["upgrade_state"]["force_upgrade"],
        false
    );
    assert_eq!(body["transient_status"]["cfg"], "");

    let mut unknown = FirmwareRouteFixture::new("firmware-execute-http-no-ack").await;
    let (prepared, _) = unknown.prepare(upgrade_metadata("no-ack")).await;
    let command_id = CommandId::parse(prepared["command_id"].as_str().unwrap()).unwrap();
    let request = unknown.spawn_json(
        Method::POST,
        "/execute",
        json!({
            "prepared_token": prepared["prepared_token"],
            "command": upgrade_command("no-ack")
        }),
    );
    let _ = unknown.next_command().await;
    unknown.published(command_id).await;
    unknown
        .typed_result(
            command_id,
            None,
            firmware_command_result::Outcome::PublishedWithoutAcknowledgement(
                PublishedWithoutAcknowledgement {},
            ),
        )
        .await
        .unwrap();
    let (status, body) = request.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["phase"], "outcome_unknown");
    assert_eq!(
        body["outcome"]["outcome"],
        "published_without_acknowledgement"
    );
    assert!(body.get("transient_status").is_none());
}

async fn assert_generation_execute_phase(published: bool, slug: &str) {
    let mut fixture = FirmwareRouteFixture::new(slug).await;
    let (prepared, _) = fixture.prepare(upgrade_metadata("execute")).await;
    let command_id = CommandId::parse(prepared["command_id"].as_str().unwrap()).unwrap();
    let request = fixture.spawn_json(
        Method::POST,
        "/execute",
        json!({
            "prepared_token": prepared["prepared_token"],
            "command": upgrade_command("execute")
        }),
    );
    let _ = fixture.next_command().await;
    if published {
        fixture.published(command_id).await;
    }
    fixture.invalidated(GENERATION + 1).await;
    let (status, body) = request.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["phase"], "outcome_unknown");
}
