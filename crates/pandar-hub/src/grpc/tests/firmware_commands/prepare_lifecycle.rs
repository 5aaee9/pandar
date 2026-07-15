use super::fixture::FirmwareFixture;
use super::support::*;
use super::*;

#[tokio::test]
async fn firmware_simultaneous_execute_with_one_token_dispatches_once() {
    let mut fixture = FirmwareFixture::new("firmware-simultaneous-execute").await;
    let prepared = fixture
        .prepare(upgrade_metadata("simultaneous-execute"))
        .await;
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let token = prepared.prepared_token.clone();
    let left = tokio::spawn(async move {
        state
            .execute_control(tenant_id, &token, upgrade_command("simultaneous-execute"))
            .await
    });
    let state = fixture.state.clone();
    let token = prepared.prepared_token.clone();
    let right = tokio::spawn(async move {
        state
            .execute_control(tenant_id, &token, upgrade_command("simultaneous-execute"))
            .await
    });

    let outbound = fixture.next_command().await;
    assert!(matches!(
        outbound.command,
        Some(hub_command::Command::ExecuteFirmwareControl(_))
    ));
    fixture
        .event(agent_event::Event::CommandResult(CommandResult {
            command_id: prepared.command_id.to_string(),
            success: false,
            error: "publish was not attempted".to_owned(),
            result_json: String::new(),
            firmware_result: None,
        }))
        .await;

    let results = [left.await.unwrap(), right.await.unwrap()];
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Ok(result) if result.phase == FirmwareExecutePhase::PrePublishFailure))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(FirmwareServiceError::InvalidPreparedToken)))
            .count(),
        1
    );
    assert!(fixture.command_receiver.try_recv().is_err());
}

#[tokio::test]
async fn firmware_session_replacement_during_prepare_cancels_exact_owner() {
    let mut fixture = FirmwareFixture::new("firmware-replacement-during-prepare").await;
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let printer_id = fixture.printer_id.clone();
    let prepare = tokio::spawn(async move {
        state
            .prepare_control(
                tenant_id,
                &printer_id,
                upgrade_metadata("replacement-during-prepare"),
                audit_actor(),
            )
            .await
    });
    let outbound = fixture.next_command().await;
    let command_id = CommandId::parse(&outbound.command_id).unwrap();
    let replaced = fixture.replace_session_without_cleanup().await;
    crate::sessions::live_commands::fail_pending_live_commands(
        &fixture.state,
        fixture.tenant_id,
        fixture.agent_id,
        replaced,
        "agent session replaced before printer operation completed",
    )
    .await;

    let error = prepare.await.unwrap().unwrap_err();
    assert!(matches!(error, FirmwareServiceError::CommandFailed { .. }));
    let command = fixture.command(command_id).await;
    assert_eq!(command.status, CommandStatus::Failed);
    assert!(command.result_json.unwrap().contains("pre_publish_failure"));
    assert!(fixture.command_receiver.try_recv().is_err());
}

#[tokio::test]
async fn firmware_lifecycle_invalid_refresh_result_remains_owned_until_disconnect_cleanup() {
    let mut fixture = FirmwareFixture::new("firmware-lifecycle-invalid-refresh").await;
    let state = fixture.state.clone();
    let printer_id = fixture.printer_id.clone();
    let tenant_id = fixture.tenant_id;
    let refresh = tokio::spawn(async move {
        state
            .refresh_version(
                tenant_id,
                &printer_id,
                "invalid-refresh".to_owned(),
                audit_actor(),
            )
            .await
    });
    let outbound = fixture.next_command().await;
    let command_id = CommandId::parse(&outbound.command_id).unwrap();
    let error = fixture
        .event_result(control_result_event(
            command_id,
            &fixture.serial,
            firmware_command_result::Outcome::RefreshedModules(FirmwareRefreshedModules {
                modules: Vec::new(),
                module_revision: u64::MAX,
            }),
        ))
        .await
        .unwrap_err();
    assert_eq!(error.code(), Code::InvalidArgument);

    fixture
        .state
        .close_agent(fixture.tenant_id, fixture.agent_id)
        .await;
    assert!(matches!(
        refresh.await.unwrap(),
        Err(FirmwareServiceError::CommandFailed { .. })
    ));
    assert_eq!(
        fixture.command(command_id).await.status,
        CommandStatus::Failed
    );
}

#[tokio::test]
async fn firmware_lifecycle_exact_prepare_expiry_does_not_cancel_newer_same_generation_entry() {
    let mut fixture = FirmwareFixture::new("firmware-lifecycle-expiry").await;
    let first = fixture.prepare(upgrade_metadata("first")).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let second = fixture.prepare(upgrade_metadata("second")).await;

    tokio::time::sleep(Duration::from_millis(550)).await;
    tokio::time::timeout(Duration::from_millis(300), async {
        loop {
            if fixture.command(first.command_id).await.status == CommandStatus::Failed {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the exact one-second prepare expiry must complete");
    assert_eq!(
        fixture.command(first.command_id).await.status,
        CommandStatus::Failed
    );
    assert_eq!(
        fixture.command(second.command_id).await.status,
        CommandStatus::Sent
    );
    assert!(matches!(
        fixture
            .state
            .execute_control(
                fixture.tenant_id,
                &first.prepared_token,
                upgrade_command("first"),
            )
            .await,
        Err(FirmwareServiceError::InvalidPreparedToken)
    ));

    let state = fixture.state.clone();
    let token = second.prepared_token.clone();
    let tenant_id = fixture.tenant_id;
    let execute = tokio::spawn(async move {
        state
            .execute_control(tenant_id, &token, upgrade_command("second"))
            .await
            .unwrap()
    });
    let outbound = fixture.next_command().await;
    assert!(matches!(
        outbound.command,
        Some(hub_command::Command::ExecuteFirmwareControl(_))
    ));
    fixture
        .event(agent_event::Event::CommandResult(CommandResult {
            command_id: second.command_id.to_string(),
            success: false,
            error: "publish failed".to_owned(),
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
async fn firmware_lifecycle_aborted_prepare_after_dispatch_still_expires_exact_owner() {
    let mut fixture = FirmwareFixture::new_file("firmware-abort-after-prepare-dispatch").await;
    let mut pause =
        crate::firmware_control::session_fence_commit_pause::install(&fixture.token.persisted_id());
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let printer_id = fixture.printer_id.clone();
    let request = tokio::spawn(async move {
        state
            .prepare_control(
                tenant_id,
                &printer_id,
                upgrade_metadata("abort-after-prepare-dispatch"),
                audit_actor(),
            )
            .await
    });
    let outbound = fixture.next_command().await;
    let command_id = CommandId::parse(&outbound.command_id).unwrap();
    assert!(matches!(
        outbound.command,
        Some(hub_command::Command::PrepareFirmwareControl(_))
    ));
    pause.wait_until_reached().await;
    assert!(
        fixture
            .state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&command_id)
    );

    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    pause.resume();
    wait_for_failed_command(&fixture, command_id).await;

    assert!(
        !fixture
            .state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&command_id)
    );
    let command = fixture.command(command_id).await;
    assert_eq!(command.status, CommandStatus::Failed);
    let persisted: crate::repositories::FirmwarePersistedResult =
        serde_json::from_str(command.result_json.as_deref().unwrap()).unwrap();
    assert_eq!(
        persisted.phase,
        crate::repositories::FirmwarePersistedPhase::PrePublishFailure
    );
}

#[tokio::test]
async fn firmware_lifecycle_aborted_prepare_before_registration_fails_without_dispatch() {
    let mut fixture = FirmwareFixture::new_file("firmware-abort-before-prepare-registration").await;
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
                upgrade_metadata("abort-before-prepare-registration"),
                audit_actor(),
            )
            .await
    });
    pause.wait_until_reached().await;
    let command = fixture.latest_command().await;
    assert!(fixture.command_receiver.try_recv().is_err());
    assert!(
        !fixture
            .state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&command.id)
    );

    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    pause.resume();
    wait_for_failed_command(&fixture, command.id).await;

    assert!(fixture.command_receiver.try_recv().is_err());
    let command = fixture.command(command.id).await;
    assert_eq!(command.status, CommandStatus::Failed);
    let persisted: crate::repositories::FirmwarePersistedResult =
        serde_json::from_str(command.result_json.as_deref().unwrap()).unwrap();
    assert_eq!(
        persisted.phase,
        crate::repositories::FirmwarePersistedPhase::PrePublishFailure
    );
}

async fn wait_for_failed_command(fixture: &FirmwareFixture, command_id: CommandId) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let command_failed = fixture.command(command_id).await.status == CommandStatus::Failed;
            let cleanup_finished = !fixture
                .state
                .sessions()
                .pending_live_command_ids()
                .await
                .contains(&command_id);
            if command_failed && cleanup_finished {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("firmware prepare expiry must complete");
}

#[tokio::test]
async fn firmware_lifecycle_expired_token_cannot_execute_while_expiry_task_waits_for_lease() {
    let mut fixture = FirmwareFixture::new("firmware-lifecycle-expired-token").await;
    let prepared = fixture.prepare(upgrade_metadata("expired-token")).await;
    let _lease = fixture
        .state
        .sessions()
        .transition_lease_for_session(fixture.agent_id, fixture.token)
        .await;

    tokio::time::sleep(Duration::from_millis(1_050)).await;
    assert!(matches!(
        fixture
            .state
            .sessions()
            .validate_firmware_execute_under_transition(
                &prepared.prepared_token,
                &upgrade_command("expired-token"),
            ),
        Err(FirmwareServiceError::InvalidPreparedToken)
    ));
}

#[tokio::test]
async fn firmware_lifecycle_terminal_result_before_execute_cannot_claim_prepared_entry() {
    let mut fixture = FirmwareFixture::new("firmware-lifecycle-early-result").await;
    let prepared = fixture.prepare(upgrade_metadata("early-result")).await;

    fixture
        .event(control_result_event(
            prepared.command_id,
            &fixture.serial,
            firmware_command_result::Outcome::Acknowledgement(FirmwareAcknowledgement {
                command: "upgrade_confirm".to_owned(),
                sequence_id: "early-result".to_owned(),
                result: Some("success".to_owned()),
                error_code: None,
                reason: None,
                message: None,
            }),
        ))
        .await;
    assert_eq!(
        fixture.command(prepared.command_id).await.status,
        CommandStatus::Sent
    );

    let execute = fixture
        .start_execute(&prepared.prepared_token, upgrade_command("early-result"))
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
