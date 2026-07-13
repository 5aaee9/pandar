use axum::http::{Method, StatusCode};
use serde_json::json;

use super::support::*;
use crate::protocol::agent::v1::hub_command;

#[tokio::test]
async fn firmware_refresh_route_is_fresh_preserves_sequence_and_commits_modules_before_success() {
    let mut fixture = FirmwareRouteFixture::new("firmware-refresh-route-fresh").await;
    fixture
        .state
        .printers()
        .replace_modules_if_current(
            fixture.tenant_id,
            fixture.agent_id,
            &fixture.token.persisted_id(),
            &fixture.serial,
            GENERATION,
            1,
            vec![module("ota", "cached")],
        )
        .await
        .unwrap();

    let request = fixture.spawn_json(
        Method::POST,
        "/refresh",
        json!({"sequence_id":"studio-refresh"}),
    );
    let outbound = fixture.next_command().await;
    let command_id = command_id(&outbound);
    let Some(hub_command::Command::RefreshFirmwareVersion(refresh)) = outbound.command else {
        panic!("expected typed refresh command");
    };
    assert_eq!(refresh.serial, fixture.serial);
    assert_eq!(refresh.sequence_id, "studio-refresh");
    assert_eq!(refresh.expected_generation, GENERATION);
    let mut fresh = proto_module("future/module", "fresh");
    fresh.software_new_version = Some("next-a".to_owned());
    fresh.new_version = Some("next-b".to_owned());
    fresh.visible = Some(false);
    fresh.product_name = Some("Future Product".to_owned());
    fresh.serial_number = Some("MODULE-SERIAL".to_owned());
    fresh.hardware_version = Some("HW-9".to_owned());
    fresh.firmware_flag = Some(5);
    fixture.refreshed(command_id, 2, vec![fresh]).await;

    let (status, body) = request.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["command_id"], command_id.to_string());
    assert_eq!(body["module_revision"], 2);
    assert_eq!(body["modules"][0]["name"], "future/module");
    assert_eq!(body["modules"][0]["sw_ver"], "fresh");
    assert_eq!(body["modules"][0]["sw_new_ver"], "next-a");
    assert_eq!(body["modules"][0]["new_ver"], "next-b");
    assert_eq!(body["modules"][0]["visible"], false);
    assert_eq!(body["modules"][0]["product_name"], "Future Product");
    assert_eq!(body["modules"][0]["sn"], "MODULE-SERIAL");
    assert_eq!(body["modules"][0]["hw_ver"], "HW-9");
    assert_eq!(body["modules"][0]["flag"], 5);
    let stored = fixture
        .state
        .printers()
        .get_with_live_status_for_tenant(fixture.tenant_id, &fixture.printer_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.firmware.module_revision, 2);
    assert_eq!(
        stored.firmware.modules.unwrap()[0]
            .software_version
            .as_deref(),
        Some("fresh")
    );
}

#[tokio::test]
async fn firmware_refresh_route_never_returns_cached_or_empty_success_after_live_failure() {
    let mut fixture = FirmwareRouteFixture::new("firmware-refresh-route-failure").await;
    fixture
        .state
        .printers()
        .replace_modules_if_current(
            fixture.tenant_id,
            fixture.agent_id,
            &fixture.token.persisted_id(),
            &fixture.serial,
            GENERATION,
            1,
            vec![module("ota", "cached-must-not-return")],
        )
        .await
        .unwrap();
    let request = fixture.spawn_json(
        Method::POST,
        "/refresh",
        json!({"sequence_id":"refresh-fail"}),
    );
    let outbound = fixture.next_command().await;
    fixture
        .generic_failure(command_id(&outbound), "bounded refresh attempts exhausted")
        .await;
    let (status, body) = request.await.unwrap();
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(body["error"], "firmware_refresh_failed");
    assert!(!body.to_string().contains("cached-must-not-return"));
}

#[tokio::test]
async fn firmware_refresh_route_rejects_empty_live_result_without_replacing_cached_modules() {
    let mut fixture = FirmwareRouteFixture::new("firmware-refresh-route-empty-result").await;
    fixture
        .state
        .printers()
        .replace_modules_if_current(
            fixture.tenant_id,
            fixture.agent_id,
            &fixture.token.persisted_id(),
            &fixture.serial,
            GENERATION,
            1,
            vec![module("ota", "cached-must-remain")],
        )
        .await
        .unwrap();

    let request = fixture.spawn_json(
        Method::POST,
        "/refresh",
        json!({"sequence_id":"refresh-empty"}),
    );
    let outbound = fixture.next_command().await;
    let command_id = command_id(&outbound);
    fixture.refreshed(command_id, 2, Vec::new()).await;

    let (status, body) = request.await.unwrap();
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(body["error"], "firmware_refresh_failed");
    assert!(!body.to_string().contains("\"modules\":[]"));

    let stored = fixture
        .state
        .printers()
        .get_with_live_status_for_tenant(fixture.tenant_id, &fixture.printer_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.firmware.module_revision, 1);
    assert_eq!(
        stored.firmware.modules.unwrap()[0]
            .software_version
            .as_deref(),
        Some("cached-must-remain")
    );
    let command = fixture
        .state
        .commands()
        .get_for_tenant(fixture.tenant_id, command_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(command.status, pandar_core::CommandStatus::Failed);
}

#[tokio::test]
async fn firmware_refresh_route_rejects_empty_module_name_and_preserves_whitespace_name() {
    let mut fixture = FirmwareRouteFixture::new("firmware-refresh-module-name").await;
    fixture
        .state
        .printers()
        .replace_modules_if_current(
            fixture.tenant_id,
            fixture.agent_id,
            &fixture.token.persisted_id(),
            &fixture.serial,
            GENERATION,
            1,
            vec![module("ota", "cached")],
        )
        .await
        .unwrap();

    let invalid_request = fixture.spawn_json(
        Method::POST,
        "/refresh",
        json!({"sequence_id":"refresh-empty-name"}),
    );
    let invalid_outbound = fixture.next_command().await;
    fixture
        .refreshed(
            command_id(&invalid_outbound),
            2,
            vec![proto_module("", "invalid")],
        )
        .await;
    let (status, body) = invalid_request.await.unwrap();
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(body["error"], "firmware_refresh_failed");
    let unchanged = fixture
        .state
        .printers()
        .get_with_live_status_for_tenant(fixture.tenant_id, &fixture.printer_id)
        .await
        .unwrap()
        .unwrap()
        .firmware;
    assert_eq!(unchanged.module_revision, 1);
    assert_eq!(unchanged.modules.unwrap()[0].name, "ota");

    let whitespace_request = fixture.spawn_json(
        Method::POST,
        "/refresh",
        json!({"sequence_id":"refresh-whitespace-name"}),
    );
    let whitespace_outbound = fixture.next_command().await;
    fixture
        .refreshed(
            command_id(&whitespace_outbound),
            2,
            vec![proto_module("   ", "preserved")],
        )
        .await;
    let (status, body) = whitespace_request.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["modules"][0]["name"], "   ");
    let stored = fixture
        .state
        .printers()
        .get_with_live_status_for_tenant(fixture.tenant_id, &fixture.printer_id)
        .await
        .unwrap()
        .unwrap()
        .firmware;
    assert_eq!(stored.module_revision, 2);
    assert_eq!(stored.modules.unwrap()[0].name, "   ");
}
