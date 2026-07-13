use super::fixture::FirmwareFixture;
use super::support::*;
use super::*;

#[tokio::test]
async fn firmware_command_redaction_keeps_url_transient_and_execute_at_most_once() {
    let mut fixture = FirmwareFixture::new("firmware-command-redaction").await;
    let metadata = start_metadata();
    let prepared = fixture.prepare(metadata.clone()).await;

    let state = fixture.state.clone();
    let command = start_command();
    let token = prepared.prepared_token.clone();
    let tenant_id = fixture.tenant_id;
    let execute = tokio::spawn(async move {
        state
            .execute_control(tenant_id, &token, command)
            .await
            .unwrap()
    });
    let outbound = fixture.next_command().await;
    let Some(hub_command::Command::ExecuteFirmwareControl(execute_command)) = outbound.command
    else {
        panic!("expected firmware execute command");
    };
    let start = execute_command
        .command
        .unwrap()
        .command
        .and_then(|command| match command {
            crate::protocol::agent::v1::firmware_command::Command::Start(start) => Some(start),
            _ => None,
        })
        .unwrap();
    assert_eq!(start.url, URL_SENTINEL);

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

    let result = execute.await.unwrap();
    assert_eq!(result.phase, FirmwareExecutePhase::Acknowledged);
    assert!(matches!(
        result.outcome,
        Some(FirmwareTerminalOutcome::Acknowledged { .. })
    ));

    let persisted = fixture.command(prepared.command_id).await;
    let audit = fixture
        .state
        .audit_events()
        .list_for_tenant(fixture.tenant_id)
        .await
        .unwrap();
    let readback = serde_json::to_string(&(persisted.clone(), audit)).unwrap();
    assert!(!readback.contains(URL_SENTINEL));
    assert_eq!(persisted.kind, "firmware_control");
    assert_eq!(persisted.status, CommandStatus::Succeeded);
    assert!(persisted.payload_json.contains("studio-sequence"));
    assert!(!persisted.payload_json.contains("main.bin"));

    let second = fixture
        .state
        .execute_control(fixture.tenant_id, &prepared.prepared_token, start_command())
        .await
        .unwrap_err();
    assert!(matches!(second, FirmwareServiceError::InvalidPreparedToken));
    assert!(fixture.command_receiver.try_recv().is_err());
}
