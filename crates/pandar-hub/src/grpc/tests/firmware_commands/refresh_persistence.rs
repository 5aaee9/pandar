use super::fixture::FirmwareFixture;
use super::support::*;
use super::*;

#[tokio::test]
async fn firmware_command_refresh_applies_generation_revision_cas_before_waiter() {
    let mut fixture = FirmwareFixture::new("firmware-command-refresh").await;
    let state = fixture.state.clone();
    let printer_id = fixture.printer_id.clone();
    let tenant_id = fixture.tenant_id;
    let refresh = tokio::spawn(async move {
        state
            .refresh_version(
                tenant_id,
                &printer_id,
                "refresh-sequence".to_owned(),
                audit_actor(),
            )
            .await
            .unwrap()
    });
    let outbound = fixture.next_command().await;
    assert!(matches!(
        outbound.command,
        Some(hub_command::Command::RefreshFirmwareVersion(_))
    ));
    let command_id = CommandId::parse(&outbound.command_id).unwrap();
    fixture
        .event(agent_event::Event::CommandAck(CommandAck {
            command_id: command_id.to_string(),
            accepted: true,
            error: String::new(),
        }))
        .await;
    fixture
        .event(control_result_event(
            command_id,
            &fixture.serial,
            firmware_command_result::Outcome::RefreshedModules(FirmwareRefreshedModules {
                modules: vec![PrinterFirmwareModule {
                    name: "ota".to_owned(),
                    software_version: Some("01.02.03".to_owned()),
                    software_new_version: None,
                    new_version: None,
                    visible: None,
                    product_name: None,
                    serial_number: None,
                    hardware_version: None,
                    firmware_flag: None,
                }],
                module_revision: 4,
            }),
        ))
        .await;

    let refreshed = tokio::time::timeout(Duration::from_millis(200), refresh)
        .await
        .expect("refresh waiter must not hold the inbound transition lease")
        .unwrap();
    assert_eq!(refreshed.module_revision, 4);
    let stored = fixture
        .state
        .printers()
        .get_with_live_status_for_tenant(fixture.tenant_id, &fixture.printer_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.firmware.module_revision, 4);
    assert_eq!(stored.firmware.modules, Some(refreshed.modules));
    assert_eq!(
        fixture.command(command_id).await.status,
        CommandStatus::Succeeded
    );
}

#[tokio::test]
async fn firmware_refresh_cas_failure_fails_waiter_and_durable_command() {
    let mut fixture = FirmwareFixture::new("firmware-refresh-cas-failure").await;
    let state = fixture.state.clone();
    let printer_id = fixture.printer_id.clone();
    let tenant_id = fixture.tenant_id;
    let refresh = tokio::spawn(async move {
        state
            .refresh_version(
                tenant_id,
                &printer_id,
                "refresh-cas-failure".to_owned(),
                audit_actor(),
            )
            .await
    });
    let outbound = fixture.next_command().await;
    let command_id = CommandId::parse(&outbound.command_id).unwrap();
    fixture
        .execute_sqlite(
            "CREATE TRIGGER fail_firmware_refresh_cas BEFORE UPDATE OF firmware_modules_json ON printers BEGIN SELECT RAISE(ABORT, 'injected firmware refresh CAS failure'); END",
        )
        .await;

    let inbound = fixture
        .event_result(control_result_event(
            command_id,
            &fixture.serial,
            firmware_command_result::Outcome::RefreshedModules(FirmwareRefreshedModules {
                modules: vec![PrinterFirmwareModule {
                    name: "ota".to_owned(),
                    software_version: Some("01.02.04".to_owned()),
                    software_new_version: None,
                    new_version: None,
                    visible: None,
                    product_name: None,
                    serial_number: None,
                    hardware_version: None,
                    firmware_flag: None,
                }],
                module_revision: 2,
            }),
        ))
        .await;
    assert_eq!(inbound.unwrap_err().code(), Code::Internal);
    let result = tokio::time::timeout(Duration::from_millis(200), refresh)
        .await
        .expect("refresh persistence failure must resolve the waiter")
        .unwrap();
    assert!(result.is_err());
    let command = fixture.command(command_id).await;
    assert_eq!(command.status, CommandStatus::Failed);
    assert!(command.result_json.unwrap().contains("pre_publish_failure"));
    assert!(
        !fixture
            .state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&command_id)
    );
}

#[tokio::test]
async fn firmware_typed_terminal_persistence_failure_is_durable_outcome_unknown() {
    let mut fixture = FirmwareFixture::new("firmware-terminal-persistence-failure").await;
    let prepared = fixture.prepare(start_metadata()).await;
    let waiter = fixture
        .start_execute(&prepared.prepared_token, start_command())
        .await;
    fixture
        .execute_sqlite(
            "CREATE TRIGGER fail_firmware_terminal_success BEFORE UPDATE OF status ON commands WHEN NEW.status = 'succeeded' BEGIN SELECT RAISE(ABORT, 'injected firmware terminal persistence failure'); END",
        )
        .await;

    let inbound = fixture
        .event_result(control_result_event(
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
    assert_eq!(inbound.unwrap_err().code(), Code::Internal);
    let result = waiter.await.unwrap();
    assert_eq!(result.phase, FirmwareExecutePhase::OutcomeUnknown);
    assert!(result.outcome.is_none());
    let command = fixture.command(prepared.command_id).await;
    assert_eq!(command.status, CommandStatus::Failed);
    let readback = serde_json::to_string(&(result, command)).unwrap();
    assert!(readback.contains("outcome_unknown"));
    assert!(!readback.contains(URL_SENTINEL));
}

#[tokio::test]
async fn firmware_terminal_and_unknown_fallback_failure_never_returns_success() {
    let mut fixture = FirmwareFixture::new("firmware-terminal-and-fallback-failure").await;
    let prepared = fixture.prepare(start_metadata()).await;
    let waiter = fixture
        .start_execute(&prepared.prepared_token, start_command())
        .await;
    fixture
        .execute_sqlite(
            "CREATE TRIGGER fail_all_firmware_terminal_updates BEFORE UPDATE OF status ON commands BEGIN SELECT RAISE(ABORT, 'injected all firmware terminal persistence failure'); END",
        )
        .await;

    let inbound = fixture
        .event_result(control_result_event(
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
    assert_eq!(inbound.unwrap_err().code(), Code::Internal);
    let result = waiter.await.unwrap();
    assert_eq!(result.phase, FirmwareExecutePhase::OutcomeUnknown);
    assert!(result.outcome.is_none());
    let command = fixture.command(prepared.command_id).await;
    assert_eq!(command.status, CommandStatus::Acknowledged);
    assert!(command.result_json.is_none());
    let pending = fixture.state.sessions().pending_live_command_ids().await;
    assert!(!pending.contains(&prepared.command_id));
    assert!(
        !serde_json::to_string(&(result, command))
            .unwrap()
            .contains(URL_SENTINEL)
    );

    fixture
        .execute_sqlite("DROP TRIGGER fail_all_firmware_terminal_updates")
        .await;
    let failed = fixture
        .state
        .commands()
        .fail_stale_unowned_live_commands(
            "2099-01-01T00:06:00Z",
            Duration::from_secs(300),
            Duration::from_secs(45),
            fixture.state.instance_id(),
            &pending,
        )
        .await
        .unwrap();
    assert_eq!(failed, 1);
    assert_eq!(
        fixture.command(prepared.command_id).await.status,
        CommandStatus::Failed
    );
}
