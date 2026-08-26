use super::fixture::FirmwareFixture;
use super::support::*;
use super::*;

#[tokio::test]
async fn firmware_lifecycle_stale_generation_invalidation_cannot_cancel_newer_command() {
    let mut fixture = FirmwareFixture::new("firmware-lifecycle-stale-invalidation").await;
    let prepared = fixture
        .prepare(upgrade_metadata("current-generation"))
        .await;

    fixture
        .event(agent_event::Event::PrinterFirmwareInvalidated(
            pandar_protocol::agent::v1::PrinterFirmwareInvalidated {
                serial: fixture.serial.clone(),
                generation: GENERATION - 1,
            },
        ))
        .await;

    assert_eq!(
        fixture.command(prepared.command_id).await.status,
        CommandStatus::Sent
    );
    let execute = fixture
        .start_execute(
            &prepared.prepared_token,
            upgrade_command("current-generation"),
        )
        .await;
    fixture
        .event(agent_event::Event::CommandResult(CommandResult {
            command_id: prepared.command_id.to_string(),
            success: false,
            error: "publish was not attempted".to_owned(),
            result_json: String::new(),
            firmware_result: None,
        }))
        .await;
    assert_eq!(
        execute.await.unwrap().phase,
        FirmwareExecutePhase::PrePublishFailure
    );
}

#[tokio::test]
async fn firmware_lifecycle_session_close_after_execute_is_outcome_unknown_and_url_free() {
    let mut fixture = FirmwareFixture::new("firmware-lifecycle-close").await;
    let prepared = fixture.prepare(start_metadata()).await;
    let state = fixture.state.clone();
    let token = prepared.prepared_token.clone();
    let tenant_id = fixture.tenant_id;
    let execute = tokio::spawn(async move {
        state
            .execute_control(tenant_id, &token, start_command())
            .await
            .unwrap()
    });
    let _ = fixture.next_command().await;

    fixture
        .state
        .close_agent(fixture.tenant_id, fixture.agent_id)
        .await;
    let result = execute.await.unwrap();
    assert_eq!(result.phase, FirmwareExecutePhase::OutcomeUnknown);
    let persisted = fixture.command(prepared.command_id).await;
    let readback = serde_json::to_string(&persisted).unwrap();
    assert_eq!(persisted.status, CommandStatus::Failed);
    assert!(!readback.contains(URL_SENTINEL));
    assert!(
        !persisted
            .error
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains("retry")
    );
}

#[tokio::test]
async fn firmware_late_negative_ack_after_publish_is_outcome_unknown() {
    let mut fixture = FirmwareFixture::new("firmware-late-negative-ack").await;
    let prepared = fixture.prepare(start_metadata()).await;
    let waiter = fixture
        .start_execute(&prepared.prepared_token, start_command())
        .await;
    fixture
        .event(agent_event::Event::FirmwarePublished(FirmwarePublished {
            command_id: prepared.command_id.to_string(),
            serial: fixture.serial.clone(),
            generation: GENERATION,
        }))
        .await;
    fixture
        .event(agent_event::Event::CommandAck(CommandAck {
            command_id: prepared.command_id.to_string(),
            accepted: false,
            error: format!("late rejection {URL_SENTINEL}"),
        }))
        .await;

    let result = waiter.await.unwrap();
    assert_eq!(result.phase, FirmwareExecutePhase::OutcomeUnknown);
    assert!(result.outcome.is_none());
    let command = fixture.command(prepared.command_id).await;
    assert_eq!(command.status, CommandStatus::Failed);
    let readback = serde_json::to_string(&(result, command)).unwrap();
    assert!(readback.contains("outcome_unknown"));
    assert!(!readback.contains("pre_publish_failure"));
    assert!(!readback.contains(URL_SENTINEL));
}

#[tokio::test]
async fn firmware_lifecycle_old_session_result_cannot_win_replacement_cleanup() {
    let mut fixture = FirmwareFixture::new("firmware-lifecycle-replacement").await;
    let prepared = fixture.prepare(upgrade_metadata("replacement")).await;
    let execute = fixture
        .start_execute(&prepared.prepared_token, upgrade_command("replacement"))
        .await;
    let replaced = fixture.replace_session_without_cleanup().await;

    fixture
        .event(control_result_event(
            prepared.command_id,
            &fixture.serial,
            firmware_command_result::Outcome::Acknowledgement(FirmwareAcknowledgement {
                command: "upgrade_confirm".to_owned(),
                sequence_id: "replacement".to_owned(),
                result: Some("success".to_owned()),
                error_code: None,
                reason: None,
                message: None,
            }),
        ))
        .await;
    crate::sessions::live_commands::fail_pending_live_commands(
        &fixture.state,
        fixture.tenant_id,
        fixture.agent_id,
        replaced,
        "agent session replaced before printer operation completed",
    )
    .await;

    assert_eq!(
        execute.await.unwrap().phase,
        FirmwareExecutePhase::OutcomeUnknown
    );
    assert_eq!(
        fixture.command(prepared.command_id).await.status,
        CommandStatus::Failed
    );
}
