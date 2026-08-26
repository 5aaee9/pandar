use super::fixture::FirmwareFixture;
use super::support::*;
use super::*;

#[tokio::test]
async fn cancelling_execute_before_dispatch_finishes_same_process_pre_publish() {
    let mut fixture = FirmwareFixture::new_file("firmware-execute-cancel-before-dispatch").await;
    let prepared = fixture
        .prepare(upgrade_metadata("cancel-before-dispatch"))
        .await;
    let mut pause =
        crate::firmware_control::session_fence_commit_pause::install(&fixture.token.persisted_id());
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let token = prepared.prepared_token.clone();
    let execute = tokio::spawn(async move {
        state
            .execute_control(tenant_id, &token, upgrade_command("cancel-before-dispatch"))
            .await
    });

    pause.wait_until_reached().await;
    assert!(fixture.command_receiver.try_recv().is_err());
    execute.abort();
    assert!(execute.await.unwrap_err().is_cancelled());
    pause.resume();

    assert!(
        fixture
            .state
            .sessions()
            .firmware_command_locator(prepared.command_id)
            .is_none()
    );
    assert!(
        fixture
            .state
            .sessions()
            .firmware_token_locator(&prepared.prepared_token)
            .is_none()
    );
    let command = wait_for_terminal(&fixture, prepared.command_id).await;
    assert!(
        !fixture
            .state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&prepared.command_id)
    );
    assert_eq!(command.status, CommandStatus::Failed);
    let persisted: crate::repositories::FirmwarePersistedResult =
        serde_json::from_str(command.result_json.as_deref().unwrap()).unwrap();
    assert_eq!(
        persisted.phase,
        crate::repositories::FirmwarePersistedPhase::PrePublishFailure
    );
    assert!(fixture.command_receiver.try_recv().is_err());
    assert!(matches!(
        fixture
            .state
            .execute_control(
                fixture.tenant_id,
                &prepared.prepared_token,
                upgrade_command("cancel-before-dispatch"),
            )
            .await,
        Err(FirmwareServiceError::InvalidPreparedToken)
    ));
}

#[tokio::test]
async fn cancelling_execute_after_dispatch_attempt_finishes_same_process_unknown() {
    let mut fixture = FirmwareFixture::new_file("firmware-execute-cancel-after-dispatch").await;
    let prepared = fixture
        .prepare(upgrade_metadata("cancel-after-dispatch"))
        .await;
    let mut durable_pause = crate::firmware_control::dispatch_ownership_pause::install(
        "execute-durable",
        &fixture.printer_id,
    );
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let token = prepared.prepared_token.clone();
    let execute = tokio::spawn(async move {
        state
            .execute_control(tenant_id, &token, upgrade_command("cancel-after-dispatch"))
            .await
    });

    durable_pause.wait_until_reached().await;
    let mut dispatch_commit_pause =
        crate::firmware_control::session_fence_commit_pause::install(&fixture.token.persisted_id());
    durable_pause.resume();
    dispatch_commit_pause.wait_until_reached().await;
    let outbound = fixture.next_command().await;
    assert!(matches!(
        outbound.command,
        Some(hub_command::Command::ExecuteFirmwareControl(_))
    ));

    execute.abort();
    assert!(execute.await.unwrap_err().is_cancelled());
    dispatch_commit_pause.resume();

    assert!(
        fixture
            .state
            .sessions()
            .firmware_command_locator(prepared.command_id)
            .is_none()
    );
    let command = wait_for_terminal(&fixture, prepared.command_id).await;
    assert!(
        !fixture
            .state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&prepared.command_id)
    );
    assert_eq!(command.status, CommandStatus::Failed);
    let persisted: crate::repositories::FirmwarePersistedResult =
        serde_json::from_str(command.result_json.as_deref().unwrap()).unwrap();
    assert_eq!(
        persisted.phase,
        crate::repositories::FirmwarePersistedPhase::OutcomeUnknown
    );
}

#[tokio::test]
async fn cancelling_execute_after_rejected_dispatch_finishes_same_process_pre_publish() {
    let mut fixture = FirmwareFixture::new_file("firmware-execute-cancel-rejected-dispatch").await;
    let prepared = fixture
        .prepare(upgrade_metadata("cancel-rejected-dispatch"))
        .await;
    let mut durable_pause = crate::firmware_control::dispatch_ownership_pause::install(
        "execute-durable",
        &fixture.printer_id,
    );
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let token = prepared.prepared_token.clone();
    let execute = tokio::spawn(async move {
        state
            .execute_control(
                tenant_id,
                &token,
                upgrade_command("cancel-rejected-dispatch"),
            )
            .await
    });

    durable_pause.wait_until_reached().await;
    fixture.close_command_channel();
    let mut dispatch_commit_pause =
        crate::firmware_control::session_fence_commit_pause::install(&fixture.token.persisted_id());
    durable_pause.resume();
    dispatch_commit_pause.wait_until_reached().await;

    execute.abort();
    assert!(execute.await.unwrap_err().is_cancelled());
    dispatch_commit_pause.resume();

    assert!(
        fixture
            .state
            .sessions()
            .firmware_command_locator(prepared.command_id)
            .is_none()
    );
    let command = wait_for_terminal(&fixture, prepared.command_id).await;
    assert!(
        !fixture
            .state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&prepared.command_id)
    );
    assert_eq!(command.status, CommandStatus::Failed);
    let persisted: crate::repositories::FirmwarePersistedResult =
        serde_json::from_str(command.result_json.as_deref().unwrap()).unwrap();
    assert_eq!(
        persisted.phase,
        crate::repositories::FirmwarePersistedPhase::PrePublishFailure
    );
}

#[tokio::test]
async fn rejected_dispatch_claim_wins_generation_cancellation_under_transition_lease() {
    let mut fixture = FirmwareFixture::new_file("firmware-execute-rejected-claim-race").await;
    let prepared = fixture
        .prepare(upgrade_metadata("rejected-claim-race"))
        .await;
    let mut durable_pause = crate::firmware_control::dispatch_ownership_pause::install(
        "execute-durable",
        &fixture.printer_id,
    );
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let token = prepared.prepared_token.clone();
    let execute = tokio::spawn(async move {
        state
            .execute_control(tenant_id, &token, upgrade_command("rejected-claim-race"))
            .await
    });

    durable_pause.wait_until_reached().await;
    fixture.close_command_channel();
    let mut claim_pause = crate::firmware_control::dispatch_ownership_pause::install(
        "execute-rejected-claim",
        &fixture.printer_id,
    );
    let mut finish_pause =
        crate::firmware_control::lifecycle_finish_pause::install(prepared.command_id);
    durable_pause.resume();
    claim_pause.wait_until_reached().await;

    let lease = fixture
        .state
        .sessions()
        .transition_lease_for_session(fixture.agent_id, fixture.token)
        .await;
    let cancelled = fixture
        .state
        .sessions()
        .cancel_firmware_generation_under_transition(
            fixture.agent_id,
            fixture.token,
            &fixture.serial,
            GENERATION + 1,
        );
    drop(lease);
    assert!(
        cancelled.is_empty(),
        "rejected dispatch cleanup must claim before releasing the transition lease"
    );

    claim_pause.resume();
    finish_pause.wait_until_reached().await;
    finish_pause.resume();
    let result = execute.await.unwrap().unwrap();
    assert_eq!(result.phase, FirmwareExecutePhase::PrePublishFailure);
    let command = wait_for_terminal(&fixture, prepared.command_id).await;
    let persisted: crate::repositories::FirmwarePersistedResult =
        serde_json::from_str(command.result_json.as_deref().unwrap()).unwrap();
    assert_eq!(
        persisted.phase,
        crate::repositories::FirmwarePersistedPhase::PrePublishFailure
    );
}

#[tokio::test]
async fn rejected_closed_dispatch_preserves_safe_cause_without_signed_url() {
    let mut fixture = FirmwareFixture::new_file("firmware-execute-rejected-closed-cause").await;
    let prepared = fixture.prepare(start_metadata()).await;
    fixture.close_command_channel();

    let result = fixture
        .state
        .execute_control(fixture.tenant_id, &prepared.prepared_token, start_command())
        .await
        .unwrap();

    assert_rejected_dispatch_cause(
        &fixture,
        prepared.command_id,
        &result,
        "firmware execute could not be sent to the current agent session: current agent command queue is closed",
    )
    .await;
}

#[tokio::test]
async fn rejected_full_dispatch_preserves_safe_cause_without_signed_url() {
    let mut fixture = FirmwareFixture::new_file("firmware-execute-rejected-full-cause").await;
    let prepared = fixture.prepare(start_metadata()).await;
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
                command_id: format!("execute-cause-filler-{index}"),
                command: None,
            }))
            .unwrap();
    }

    let result = fixture
        .state
        .execute_control(fixture.tenant_id, &prepared.prepared_token, start_command())
        .await
        .unwrap();

    assert_rejected_dispatch_cause(
        &fixture,
        prepared.command_id,
        &result,
        "firmware execute could not be sent to the current agent session: current agent command queue is full",
    )
    .await;
}

async fn assert_rejected_dispatch_cause(
    fixture: &FirmwareFixture,
    command_id: CommandId,
    result: &crate::firmware_control::FirmwareExecuteResult,
    expected: &str,
) {
    assert_eq!(result.phase, FirmwareExecutePhase::PrePublishFailure);
    assert_eq!(result.error.as_deref(), Some(expected));
    assert!(!result.error.as_deref().unwrap().contains(URL_SENTINEL));

    let command = wait_for_terminal(fixture, command_id).await;
    assert_eq!(command.status, CommandStatus::Failed);
    assert_eq!(command.error.as_deref(), Some(expected));
    assert!(!command.payload_json.contains(URL_SENTINEL));
    assert!(!command.error.as_deref().unwrap().contains(URL_SENTINEL));
    assert!(
        !command
            .result_json
            .as_deref()
            .unwrap()
            .contains(URL_SENTINEL)
    );
}

async fn wait_for_terminal(
    fixture: &FirmwareFixture,
    command_id: CommandId,
) -> pandar_core::CommandRecord {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let command = fixture.command(command_id).await;
            if matches!(
                command.status,
                CommandStatus::Succeeded | CommandStatus::Failed
            ) && !fixture
                .state
                .sessions()
                .pending_live_command_ids()
                .await
                .contains(&command_id)
            {
                return command;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancelled firmware execute must persist a terminal result")
}
