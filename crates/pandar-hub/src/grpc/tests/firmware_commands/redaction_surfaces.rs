use super::fixture::FirmwareFixture;
use super::support::*;
use super::*;

mod capacity;
mod retention;

#[tokio::test]
async fn firmware_redaction_agent_error_echo_never_reaches_durable_readback() {
    let mut fixture = FirmwareFixture::new("firmware-redaction-error").await;
    let prepared = fixture.prepare(start_metadata()).await;
    let waiter = fixture
        .start_execute(&prepared.prepared_token, start_command())
        .await;
    fixture
        .event(agent_event::Event::CommandResult(CommandResult {
            command_id: prepared.command_id.to_string(),
            success: false,
            error: format!("publish rejected url={URL_SENTINEL}"),
            result_json: String::new(),
            firmware_result: None,
        }))
        .await;
    let result = waiter.await.unwrap();
    assert_eq!(result.phase, FirmwareExecutePhase::PrePublishFailure);
    let readback = serde_json::to_string(&fixture.command(prepared.command_id).await).unwrap();
    assert!(!readback.contains(URL_SENTINEL));
    assert!(readback.contains("[redacted]"));
}

#[tokio::test]
async fn firmware_redaction_typed_ack_echo_never_reaches_result_or_durable_readback() {
    let mut fixture = FirmwareFixture::new("firmware-redaction-typed-ack").await;
    let prepared = fixture.prepare(start_metadata()).await;
    let waiter = fixture
        .start_execute(&prepared.prepared_token, start_command())
        .await;
    fixture
        .event(control_result_event(
            prepared.command_id,
            &fixture.serial,
            firmware_command_result::Outcome::Acknowledgement(FirmwareAcknowledgement {
                command: "start".to_owned(),
                sequence_id: "studio-sequence".to_owned(),
                result: Some("fail".to_owned()),
                error_code: Some(9),
                reason: Some(format!("rejected package {URL_SENTINEL}")),
                message: Some(format!("cannot load {URL_SENTINEL}")),
            }),
        ))
        .await;
    let result = waiter.await.unwrap();
    let result_json = serde_json::to_string(&result).unwrap();
    let durable_json = serde_json::to_string(&fixture.command(prepared.command_id).await).unwrap();
    assert!(!result_json.contains(URL_SENTINEL));
    assert!(!durable_json.contains(URL_SENTINEL));
    assert!(result_json.contains("[redacted]"));
    assert!(durable_json.contains("[redacted]"));
}

#[tokio::test]
async fn firmware_redaction_scrubs_every_typed_snapshot_and_result_string_surface() {
    let mut fixture = FirmwareFixture::new("firmware-redaction-all-typed-surfaces").await;
    let prepared = fixture.prepare(start_metadata()).await;
    let waiter = fixture
        .start_execute(&prepared.prepared_token, start_command())
        .await;

    fixture
        .event(agent_event::Event::PrinterFirmwareModulesSnapshot(
            PrinterFirmwareModulesSnapshot {
                serial: fixture.serial.clone(),
                generation: GENERATION,
                module_revision: 1,
                modules: vec![leaking_module()],
            },
        ))
        .await;
    fixture
        .event(agent_event::Event::PrinterFirmwareStatusSnapshot(
            PrinterFirmwareStatusSnapshot {
                serial: fixture.serial.clone(),
                generation: GENERATION,
                status_revision: 1,
                upgrade_state: Some(leaking_upgrade_state()),
                cfg: Some(leaking_value("cfg")),
            },
        ))
        .await;
    fixture
        .event(agent_event::Event::CommandAck(CommandAck {
            command_id: prepared.command_id.to_string(),
            accepted: true,
            error: String::new(),
        }))
        .await;
    fixture
        .event(agent_event::Event::FirmwarePublished(FirmwarePublished {
            command_id: prepared.command_id.to_string(),
            serial: fixture.serial.clone(),
            generation: GENERATION,
        }))
        .await;
    fixture
        .event(control_result_event_with_status(
            prepared.command_id,
            &fixture.serial,
            PrinterFirmwareStatus {
                upgrade_state: Some(leaking_upgrade_state()),
                cfg: Some(leaking_value("result-cfg")),
            },
            firmware_command_result::Outcome::Acknowledgement(FirmwareAcknowledgement {
                command: leaking_value("ack-command"),
                sequence_id: leaking_value("ack-sequence"),
                result: Some(leaking_value("ack-result")),
                error_code: Some(0),
                reason: Some(leaking_value("ack-reason")),
                message: Some(leaking_value("ack-message")),
            }),
        ))
        .await;

    let result = waiter.await.unwrap();
    let firmware = fixture
        .state
        .printers()
        .get_with_live_status_for_tenant(fixture.tenant_id, &fixture.printer_id)
        .await
        .unwrap()
        .unwrap()
        .firmware;
    let readback = serde_json::to_string(&(
        firmware.clone(),
        result.clone(),
        fixture.command(prepared.command_id).await,
    ))
    .unwrap();
    for forbidden in [
        URL_SENTINEL,
        "FIRMWARE-URL-SENTINEL",
        "/main.bin",
        "user",
        "secret",
    ] {
        assert!(
            !readback.contains(forbidden),
            "leaked {forbidden}: {readback}"
        );
    }
    assert_eq!(firmware.modules.unwrap()[0].firmware_flag, Some(17));
    let stored_ams = firmware.upgrade_state.unwrap().ams_firmware.unwrap();
    assert_eq!(stored_ams.firmware.unwrap()[0].id, 41);
    assert_eq!(stored_ams.current_firmware_id, Some(42));
    assert_eq!(stored_ams.current_run_firmware_id, Some(43));
    let result_ams = result
        .transient_status
        .unwrap()
        .upgrade_state
        .unwrap()
        .ams_firmware
        .unwrap();
    assert_eq!(result_ams.firmware.unwrap()[0].id, 41);
    assert_eq!(result_ams.current_firmware_id, Some(42));
    assert_eq!(result_ams.current_run_firmware_id, Some(43));
}

#[tokio::test]
async fn firmware_snapshot_redaction_scrubs_all_matching_live_command_urls() {
    let mut fixture = FirmwareFixture::new("firmware-redaction-multiple-live-urls").await;
    let first = fixture.prepare(start_metadata()).await;
    let second = fixture.prepare(second_start_metadata()).await;
    let first_waiter = fixture
        .start_execute(&first.prepared_token, start_command())
        .await;
    let second_waiter = fixture
        .start_execute(&second.prepared_token, second_start_command())
        .await;

    let mut module = module_with_version("ota", "01.02.03");
    module.name = format!("{URL_SENTINEL} {SECOND_URL_SENTINEL}");
    fixture
        .event(agent_event::Event::PrinterFirmwareModulesSnapshot(
            PrinterFirmwareModulesSnapshot {
                serial: fixture.serial.clone(),
                generation: GENERATION,
                module_revision: 1,
                modules: vec![module],
            },
        ))
        .await;
    let readback = serde_json::to_string(
        &fixture
            .state
            .printers()
            .get_with_live_status_for_tenant(fixture.tenant_id, &fixture.printer_id)
            .await
            .unwrap()
            .unwrap()
            .firmware,
    )
    .unwrap();
    for forbidden in [
        "/main.bin",
        "FIRMWARE-URL-SENTINEL",
        "SECOND-USER",
        "SECOND-PASSWORD",
        "FIRMWARE-SECOND-PATH",
        "FIRMWARE-SECOND-QUERY",
    ] {
        assert!(
            !readback.contains(forbidden),
            "leaked {forbidden}: {readback}"
        );
    }

    for command_id in [first.command_id, second.command_id] {
        fixture
            .event(agent_event::Event::CommandResult(CommandResult {
                command_id: command_id.to_string(),
                success: false,
                error: "publish was not attempted".to_owned(),
                result_json: String::new(),
                firmware_result: None,
            }))
            .await;
    }
    assert_eq!(
        first_waiter.await.unwrap().phase,
        FirmwareExecutePhase::PrePublishFailure
    );
    assert_eq!(
        second_waiter.await.unwrap().phase,
        FirmwareExecutePhase::PrePublishFailure
    );
}

#[tokio::test]
async fn firmware_snapshot_redaction_retains_completed_command_url_after_completion() {
    let mut fixture = FirmwareFixture::new("firmware-redaction-completed-url").await;
    let prepared = fixture.prepare(start_metadata()).await;
    let waiter = fixture
        .start_execute(&prepared.prepared_token, start_command())
        .await;
    fixture
        .event(control_result_event(
            prepared.command_id,
            &fixture.serial,
            firmware_command_result::Outcome::Acknowledgement(FirmwareAcknowledgement {
                command: "start".to_owned(),
                sequence_id: "studio-sequence".to_owned(),
                result: Some("success".to_owned()),
                error_code: Some(0),
                reason: None,
                message: None,
            }),
        ))
        .await;
    assert_eq!(
        waiter.await.unwrap().phase,
        FirmwareExecutePhase::Acknowledged
    );
    assert!(
        !fixture
            .state
            .sessions()
            .pending_firmware_command_ids()
            .contains(&prepared.command_id)
    );

    fixture
        .event(agent_event::Event::PrinterFirmwareModulesSnapshot(
            PrinterFirmwareModulesSnapshot {
                serial: fixture.serial.clone(),
                generation: GENERATION,
                module_revision: 1,
                modules: vec![leaking_module()],
            },
        ))
        .await;
    fixture
        .event(agent_event::Event::PrinterFirmwareStatusSnapshot(
            PrinterFirmwareStatusSnapshot {
                serial: fixture.serial.clone(),
                generation: GENERATION,
                status_revision: 1,
                upgrade_state: Some(leaking_upgrade_state()),
                cfg: Some(leaking_value("post-terminal-cfg")),
            },
        ))
        .await;

    let readback = serde_json::to_string(
        &fixture
            .state
            .printers()
            .get_with_live_status_for_tenant(fixture.tenant_id, &fixture.printer_id)
            .await
            .unwrap()
            .unwrap()
            .firmware,
    )
    .unwrap();
    for forbidden in [
        URL_SENTINEL,
        "FIRMWARE-URL-SENTINEL",
        "/main.bin",
        "user",
        "secret",
    ] {
        assert!(
            !readback.contains(forbidden),
            "completed command URL leaked {forbidden}: {readback}"
        );
    }
}
