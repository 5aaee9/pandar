use super::fixture::FirmwareFixture;
use super::support::*;
use super::*;

#[tokio::test]
async fn firmware_lifecycle_non_owner_and_generation_change_never_replay_or_claim_late_result() {
    let mut fixture = FirmwareFixture::new("firmware-lifecycle-owner").await;
    let sibling = fixture.state.sibling_for_tests();
    let before = fixture.state.commands().count().await.unwrap();
    let error = sibling
        .prepare_control(
            fixture.tenant_id,
            &fixture.printer_id,
            upgrade_metadata("non-owner"),
            audit_actor(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, FirmwareServiceError::Unavailable));
    assert_eq!(fixture.state.commands().count().await.unwrap(), before);

    let old = fixture.prepare(upgrade_metadata("old")).await;
    assert!(matches!(
        sibling
            .execute_control(
                fixture.tenant_id,
                &old.prepared_token,
                upgrade_command("old"),
            )
            .await,
        Err(FirmwareServiceError::InvalidPreparedToken)
    ));
    fixture
        .event(agent_event::Event::PrinterFirmwareInvalidated(
            pandar_protocol::agent::v1::PrinterFirmwareInvalidated {
                serial: fixture.serial.clone(),
                generation: GENERATION + 1,
            },
        ))
        .await;
    assert!(matches!(
        fixture
            .state
            .execute_control(
                fixture.tenant_id,
                &old.prepared_token,
                upgrade_command("old"),
            )
            .await,
        Err(FirmwareServiceError::InvalidPreparedToken)
    ));
    let terminal = fixture.command(old.command_id).await;
    assert_eq!(terminal.status, CommandStatus::Failed);

    fixture
        .event(control_result_event(
            old.command_id,
            &fixture.serial,
            firmware_command_result::Outcome::Acknowledgement(FirmwareAcknowledgement {
                command: "upgrade_confirm".to_owned(),
                sequence_id: "old".to_owned(),
                result: Some("success".to_owned()),
                error_code: None,
                reason: None,
                message: None,
            }),
        ))
        .await;
    assert_eq!(fixture.command(old.command_id).await, terminal);
    assert!(fixture.command_receiver.try_recv().is_err());
}

#[tokio::test]
async fn firmware_cross_hub_replacement_blocks_stale_prepare() {
    let mut fixture = FirmwareFixture::new("firmware-cross-hub-prepare").await;
    fixture.claim_authoritative_sibling_session().await;

    let error = tokio::time::timeout(
        Duration::from_millis(100),
        fixture.state.prepare_control(
            fixture.tenant_id,
            &fixture.printer_id,
            upgrade_metadata("stale-prepare"),
            audit_actor(),
        ),
    )
    .await
    .expect("stale Hub prepare must fail before dispatch")
    .unwrap_err();
    assert!(matches!(error, FirmwareServiceError::Unavailable));
    let command = fixture.latest_command().await;
    assert_eq!(command.status, CommandStatus::Failed);
    assert!(command.result_json.unwrap().contains("pre_publish_failure"));
    assert!(fixture.command_receiver.try_recv().is_err());
}

#[tokio::test]
async fn firmware_cross_hub_replacement_blocks_stale_refresh() {
    let mut fixture = FirmwareFixture::new("firmware-cross-hub-refresh").await;
    fixture.claim_authoritative_sibling_session().await;

    let error = tokio::time::timeout(
        Duration::from_millis(100),
        fixture.state.refresh_version(
            fixture.tenant_id,
            &fixture.printer_id,
            "stale-refresh".to_owned(),
            audit_actor(),
        ),
    )
    .await
    .expect("stale Hub refresh must fail before dispatch")
    .unwrap_err();
    assert!(matches!(error, FirmwareServiceError::Unavailable));
    let command = fixture.latest_command().await;
    assert_eq!(command.status, CommandStatus::Failed);
    assert!(command.result_json.unwrap().contains("pre_publish_failure"));
    assert!(fixture.command_receiver.try_recv().is_err());
}

#[tokio::test]
async fn firmware_cross_hub_replacement_blocks_stale_execute() {
    let mut fixture = FirmwareFixture::new("firmware-cross-hub-execute").await;
    let prepared = fixture.prepare(upgrade_metadata("stale-execute")).await;
    let command_before = fixture.command(prepared.command_id).await;
    fixture.claim_authoritative_sibling_session().await;

    let error = tokio::time::timeout(
        Duration::from_millis(100),
        fixture.state.execute_control(
            fixture.tenant_id,
            &prepared.prepared_token,
            upgrade_command("stale-execute"),
        ),
    )
    .await
    .expect("stale Hub execute must fail before dispatch")
    .unwrap_err();
    assert!(matches!(error, FirmwareServiceError::Unavailable));
    let command = fixture.command(prepared.command_id).await;
    assert_eq!(command, command_before);
    assert!(fixture.command_receiver.try_recv().is_err());

    fixture
        .state
        .agents()
        .claim_online_session(
            fixture.tenant_id,
            fixture.agent_id,
            &fixture.token.persisted_id(),
            "firmware-test-restored",
            "2026-07-12T00:00:03Z",
        )
        .await
        .unwrap();
    let waiter = fixture
        .start_execute(&prepared.prepared_token, upgrade_command("stale-execute"))
        .await;
    fixture
        .event(agent_event::Event::CommandResult(CommandResult {
            command_id: prepared.command_id.to_string(),
            success: false,
            error: "known before publish".to_owned(),
            result_json: String::new(),
            firmware_result: None,
        }))
        .await;
    assert_eq!(
        waiter.await.unwrap().phase,
        FirmwareExecutePhase::PrePublishFailure
    );
}

#[tokio::test]
async fn firmware_cross_hub_replacement_ignores_stale_typed_completion() {
    let mut fixture = FirmwareFixture::new("firmware-cross-hub-completion").await;
    let prepared = fixture.prepare(upgrade_metadata("stale-completion")).await;
    let waiter = fixture
        .start_execute(
            &prepared.prepared_token,
            upgrade_command("stale-completion"),
        )
        .await;
    fixture.claim_authoritative_sibling_session().await;

    fixture
        .event(control_result_event(
            prepared.command_id,
            &fixture.serial,
            firmware_command_result::Outcome::Acknowledgement(FirmwareAcknowledgement {
                command: "upgrade_confirm".to_owned(),
                sequence_id: "stale-completion".to_owned(),
                result: Some("success".to_owned()),
                error_code: Some(0),
                reason: None,
                message: None,
            }),
        ))
        .await;

    tokio::task::yield_now().await;
    assert!(!waiter.is_finished());
    let command = fixture.command(prepared.command_id).await;
    assert_eq!(command.status, CommandStatus::Acknowledged);
    assert!(command.result_json.is_none());
    waiter.abort();
}

#[tokio::test]
async fn firmware_cross_hub_session_fence_serializes_dispatch_before_replacement() {
    let mut fixture = FirmwareFixture::new_file("firmware-cross-hub-dispatch-fence").await;
    let mut pause =
        crate::repositories::current_transaction_pause::install(&fixture.token.persisted_id());
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let printer_id = fixture.printer_id.clone();
    let request = tokio::spawn(async move {
        state
            .prepare_control(
                tenant_id,
                &printer_id,
                upgrade_metadata("fenced-dispatch"),
                audit_actor(),
            )
            .await
    });
    pause.wait_until_reached().await;
    assert!(fixture.command_receiver.try_recv().is_err());

    let sibling = fixture.state.sibling_for_tests();
    let agent_id = fixture.agent_id;
    let replacement_token = SessionToken::new();
    let mut replacement = tokio::spawn(async move {
        sibling
            .agents()
            .claim_online_session(
                tenant_id,
                agent_id,
                &replacement_token.persisted_id(),
                "replacement",
                "2026-07-12T00:00:03Z",
            )
            .await
            .unwrap();
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut replacement)
            .await
            .is_err(),
        "replacement must wait for the authoritative dispatch fence"
    );

    pause.resume();
    let outbound = tokio::select! {
        biased;
        outbound = fixture.command_receiver.recv() => outbound
            .expect("firmware command channel closed")
            .expect("firmware command status"),
        result = &mut replacement => panic!("replacement completed before dispatch: {result:?}"),
    };
    assert!(matches!(
        outbound.command,
        Some(hub_command::Command::PrepareFirmwareControl(_))
    ));
    tokio::time::timeout(Duration::from_secs(1), &mut replacement)
        .await
        .expect("replacement must complete after dispatch fence release")
        .unwrap();
    request.abort();
    let _ = request.await;
}
