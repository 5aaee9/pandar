use super::fixture::FirmwareFixture;
use super::support::*;
use super::*;

#[tokio::test]
async fn firmware_command_incapable_session_is_unavailable_without_durable_record() {
    let fixture = FirmwareFixture::new("firmware-command-incapable").await;
    let mut session = fixture
        .state
        .sessions()
        .get(fixture.agent_id)
        .await
        .unwrap();
    session.capabilities.clear();
    fixture.state.sessions().register(session).await;
    let before = fixture.state.commands().count().await.unwrap();
    let error = fixture
        .state
        .prepare_control(
            fixture.tenant_id,
            &fixture.printer_id,
            upgrade_metadata("incapable"),
            audit_actor(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, FirmwareServiceError::Unavailable));
    assert_eq!(fixture.state.commands().count().await.unwrap(), before);
}

#[tokio::test]
async fn firmware_command_late_generic_result_has_no_durable_fallback() {
    let fixture = FirmwareFixture::new("firmware-command-no-fallback").await;
    for kind in ["firmware_refresh", "firmware_control"] {
        let command_id = CommandId::new();
        fixture
            .insert_raw_command(command_id, kind, CommandStatus::Sent)
            .await;
        fixture
            .event(agent_event::Event::CommandResult(CommandResult {
                command_id: command_id.to_string(),
                success: false,
                error: "late result".to_owned(),
                result_json: "{\"late\":true}".to_owned(),
                firmware_result: None,
            }))
            .await;
        let command = fixture.command(command_id).await;
        assert_eq!(command.status, CommandStatus::Sent);
        assert!(command.error.is_none());
        assert!(command.result_json.is_none());
    }
}

#[tokio::test]
async fn firmware_lifecycle_close_while_prepare_waits_is_exact_pre_publish_failure() {
    let mut fixture = FirmwareFixture::new("firmware-lifecycle-prepare-close").await;
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let printer_id = fixture.printer_id.clone();
    let prepare = tokio::spawn(async move {
        state
            .prepare_control(
                tenant_id,
                &printer_id,
                upgrade_metadata("close-before-prepared"),
                audit_actor(),
            )
            .await
    });
    let outbound = fixture.next_command().await;
    let command_id = CommandId::parse(&outbound.command_id).unwrap();
    fixture
        .state
        .close_agent(fixture.tenant_id, fixture.agent_id)
        .await;
    assert!(matches!(
        prepare.await.unwrap(),
        Err(FirmwareServiceError::CommandFailed { .. })
    ));
    assert_eq!(
        fixture.command(command_id).await.status,
        CommandStatus::Failed
    );
}

#[tokio::test]
async fn firmware_lifecycle_prepare_wait_is_bounded_while_expiry_cleanup_waits_for_lease() {
    let mut fixture = FirmwareFixture::new("firmware-lifecycle-bounded-prepare").await;
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let printer_id = fixture.printer_id.clone();
    let prepare = tokio::spawn(async move {
        state
            .prepare_control(
                tenant_id,
                &printer_id,
                upgrade_metadata("bounded-prepare"),
                audit_actor(),
            )
            .await
    });
    let _ = fixture.next_command().await;
    let _lease = fixture
        .state
        .sessions()
        .transition_lease_for_session(fixture.agent_id, fixture.token)
        .await;

    let result = tokio::time::timeout(Duration::from_millis(1_200), prepare)
        .await
        .expect("firmware prepare must stop waiting at its one-second deadline")
        .unwrap();
    assert!(matches!(
        result,
        Err(FirmwareServiceError::CommandFailed { .. })
    ));
}

#[tokio::test]
async fn firmware_command_execute_full_channel_is_known_pre_publish_failure() {
    let mut fixture = FirmwareFixture::new("firmware-command-execute-full").await;
    let prepared = fixture.prepare(upgrade_metadata("execute-full")).await;
    let session = fixture
        .state
        .sessions()
        .get(fixture.agent_id)
        .await
        .unwrap();
    for index in 0..8 {
        session
            .command_sender
            .try_send(Ok(pandar_protocol::agent::v1::HubCommand {
                command_id: format!("execute-filler-{index}"),
                command: None,
            }))
            .unwrap();
    }
    let result = fixture
        .state
        .execute_control(
            fixture.tenant_id,
            &prepared.prepared_token,
            upgrade_command("execute-full"),
        )
        .await
        .unwrap();
    assert_eq!(result.phase, FirmwareExecutePhase::PrePublishFailure);
    assert_eq!(
        fixture.command(prepared.command_id).await.status,
        CommandStatus::Failed
    );
}
