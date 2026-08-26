use axum::http::{Method, StatusCode};
use pandar_core::CommandId;
use serde_json::{Value, json};

use super::super::*;
use super::support::*;
use pandar_protocol::agent::v1::{firmware_command, hub_command};

#[tokio::test]
async fn firmware_prepare_is_url_free_and_execute_is_typed_exact_and_one_use() {
    let mut fixture = FirmwareRouteFixture::new("firmware-prepare-execute-exact").await;
    let (prepared, outbound) = fixture
        .prepare(start_metadata(
            "opaque-sequence",
            "future/module name",
            "version with spaces",
        ))
        .await;
    let Some(hub_command::Command::PrepareFirmwareControl(prepare)) = outbound.command else {
        panic!("expected firmware prepare");
    };
    assert_eq!(prepare.serial, fixture.serial);
    assert_eq!(prepare.expected_generation, GENERATION);
    assert!(!format!("{prepare:?}").contains(URL_SENTINEL));
    let prepared_token = prepared["prepared_token"].as_str().unwrap().to_owned();
    let command_id = CommandId::parse(prepared["command_id"].as_str().unwrap()).unwrap();

    let execute = fixture.spawn_json(
        Method::POST,
        "/execute",
        json!({
            "prepared_token": prepared_token,
            "command": start_command(
                "opaque-sequence",
                URL_SENTINEL,
                "future/module name",
                "version with spaces"
            )
        }),
    );
    let outbound = fixture.next_command().await;
    let Some(hub_command::Command::ExecuteFirmwareControl(execute_command)) = outbound.command
    else {
        panic!("expected firmware execute");
    };
    let command = execute_command.command.unwrap();
    let Some(firmware_command::Command::Start(start)) = command.command else {
        panic!("expected typed start");
    };
    assert_eq!(command.sequence_id, "opaque-sequence");
    assert_eq!(start.url, URL_SENTINEL);
    assert_eq!(start.module, "future/module name");
    assert_eq!(start.version, "version with spaces");
    fixture.published(command_id).await;
    fixture
        .acknowledged(command_id, "start", "opaque-sequence")
        .await;
    let (status, body) = execute.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["phase"], "acknowledged");
    assert_eq!(body["outcome"]["outcome"], "acknowledged");
    assert_eq!(body["outcome"]["acknowledgement"]["command"], "start");
    assert_eq!(
        body["outcome"]["acknowledgement"]["sequence_id"],
        "opaque-sequence"
    );
    assert_eq!(body["outcome"]["acknowledgement"]["result"], "success");
    assert_eq!(body["outcome"]["acknowledgement"]["err_code"], 0);
    assert!(!body.to_string().contains(URL_SENTINEL));
    assert!(!body.to_string().contains("user:secret"));

    let (status, body) = request_as(
        fixture.app(),
        Method::POST,
        &fixture.uri("/execute"),
        Some(json!({
            "prepared_token": prepared["prepared_token"],
            "command": start_command(
                "opaque-sequence",
                URL_SENTINEL,
                "future/module name",
                "version with spaces"
            )
        })),
        &fixture.auth,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["phase"], "pre_publish_failure");
    assert!(fixture.commands.try_recv().is_err());
}

#[tokio::test]
async fn firmware_prepare_and_execute_accept_all_closed_variants_without_extra_product_policy() {
    let mut fixture = FirmwareRouteFixture::new("firmware-prepare-variants").await;
    let cases = [
        (
            upgrade_metadata("upgrade"),
            upgrade_command("upgrade"),
            "upgrade_confirm",
        ),
        (
            json!({"command":"consistency_confirm","sequence_id":"consistent","src_id":-8}),
            json!({"command":"consistency_confirm","sequence_id":"consistent","src_id":-8}),
            "consistency_confirm",
        ),
        (
            start_metadata("start", "unknown/future", "v custom"),
            start_command(
                "start",
                "not-a-web-url:opaque",
                "unknown/future",
                "v custom",
            ),
            "start",
        ),
        (
            json!({"command":"mc_for_ams_firmware_upgrade","sequence_id":"ams","src_id":1,"id":-3}),
            json!({"command":"mc_for_ams_firmware_upgrade","sequence_id":"ams","src_id":1,"id":-3}),
            "mc_for_ams_firmware_upgrade",
        ),
    ];
    for (metadata, command, expected) in cases {
        let expected_sequence = command["sequence_id"].as_str().unwrap().to_owned();
        let expected_src_id = command["src_id"].as_i64().unwrap();
        let expected_ams_id = command.get("id").and_then(Value::as_i64);
        let (prepared, _) = fixture.prepare(metadata).await;
        let request = fixture.spawn_json(
            Method::POST,
            "/execute",
            json!({
                "prepared_token": prepared["prepared_token"],
                "command": command,
            }),
        );
        let outbound = fixture.next_command().await;
        let id = command_id(&outbound);
        let Some(hub_command::Command::ExecuteFirmwareControl(execute)) = outbound.command else {
            panic!("expected execute");
        };
        let proto = execute.command.unwrap();
        assert_eq!(proto.sequence_id, expected_sequence);
        assert_eq!(proto.src_id, expected_src_id);
        let actual = match proto.command.unwrap() {
            firmware_command::Command::UpgradeConfirm(_) => "upgrade_confirm",
            firmware_command::Command::ConsistencyConfirm(_) => "consistency_confirm",
            firmware_command::Command::Start(_) => "start",
            firmware_command::Command::SwitchAmsFirmware(command) => {
                assert_eq!(i64::from(command.id), expected_ams_id.unwrap());
                "mc_for_ams_firmware_upgrade"
            }
        };
        assert_eq!(actual, expected);
        fixture.generic_failure(id, "known before publish").await;
        let (status, body) = request.await.unwrap();
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["phase"], "pre_publish_failure");
    }
}

#[tokio::test]
async fn firmware_execute_metadata_mismatch_is_explicit_safe_and_consumes_token() {
    let mut fixture = FirmwareRouteFixture::new("firmware-execute-metadata-mismatch").await;
    let (prepared, _) = fixture.prepare(upgrade_metadata("original")).await;
    for expected_error in [
        "firmware_metadata_mismatch",
        "invalid_firmware_prepared_token",
    ] {
        let (status, body) = request_as(
            fixture.app(),
            Method::POST,
            &fixture.uri("/execute"),
            Some(json!({
                "prepared_token": prepared["prepared_token"],
                "command": upgrade_command("different"),
            })),
            &fixture.auth,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], expected_error);
        assert_eq!(body["phase"], "pre_publish_failure");
    }
    assert!(fixture.commands.try_recv().is_err());
}

#[tokio::test]
async fn firmware_execute_token_is_bound_to_the_exact_path_printer_without_consumption() {
    let mut fixture = FirmwareRouteFixture::new("firmware-execute-path-binding").await;
    let (prepared, _) = fixture.prepare(upgrade_metadata("path-bound")).await;
    let other_agent = feature_advertisement_printer(
        &fixture.state,
        fixture.tenant_id,
        "path-other-agent",
        "path-other-printer",
    )
    .await;
    let other_printer = fixture
        .state
        .printers()
        .list_with_live_status_for_tenant(fixture.tenant_id)
        .await
        .unwrap()
        .into_iter()
        .find(|printer| printer.printer.agent_id == other_agent)
        .unwrap()
        .printer;
    let body = json!({
        "prepared_token": prepared["prepared_token"],
        "command": upgrade_command("path-bound")
    });
    for (printer_id, expected_status, expected_error) in [
        (
            "missing".to_owned(),
            StatusCode::NOT_FOUND,
            "printer_not_found",
        ),
        (
            other_printer.id,
            StatusCode::CONFLICT,
            "invalid_firmware_prepared_token",
        ),
    ] {
        let (status, response) = request_as(
            fixture.app(),
            Method::POST,
            &format!("/api/v1/plugin/printers/{printer_id}/firmware/execute"),
            Some(body.clone()),
            &fixture.auth,
        )
        .await;
        assert_eq!(status, expected_status);
        assert_eq!(response["error"], expected_error);
        assert_eq!(response["phase"], "pre_publish_failure");
        assert!(fixture.commands.try_recv().is_err());
    }

    let (status, response) = request_as(
        fixture.app(),
        Method::POST,
        &fixture.uri("/execute"),
        Some(json!({
            "prepared_token": "random-owning-hub-token",
            "command": upgrade_command("path-bound")
        })),
        &fixture.auth,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(response["error"], "invalid_firmware_prepared_token");
    assert_eq!(response["phase"], "pre_publish_failure");
    assert!(fixture.commands.try_recv().is_err());

    let request = fixture.spawn_json(Method::POST, "/execute", body);
    let outbound = fixture.next_command().await;
    fixture
        .generic_failure(command_id(&outbound), "known before publish")
        .await;
    let (status, body) = request.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["phase"], "pre_publish_failure");
}
