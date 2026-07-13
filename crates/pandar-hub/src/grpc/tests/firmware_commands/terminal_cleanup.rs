use super::fixture::FirmwareFixture;
use super::support::*;
use super::*;

#[tokio::test]
async fn firmware_double_terminal_failure_is_cleanup_eligible_with_healthy_exact_session() {
    let mut fixture = FirmwareFixture::new_file("firmware-double-terminal-local-cleanup").await;
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
    assert_eq!(
        waiter.await.unwrap().phase,
        FirmwareExecutePhase::OutcomeUnknown
    );
    assert!(
        !fixture
            .state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&prepared.command_id)
    );
    fixture
        .execute_sqlite("DROP TRIGGER fail_all_firmware_terminal_updates")
        .await;
    fixture
        .set_command_updated_at(prepared.command_id, "2026-07-12T00:00:00Z")
        .await;
    fixture
        .state
        .agents()
        .heartbeat_if_current(
            fixture.tenant_id,
            fixture.agent_id,
            &fixture.token.persisted_id(),
            "2026-07-12T00:06:00Z",
        )
        .await
        .unwrap();

    let failed = fixture
        .state
        .commands()
        .fail_stale_unowned_live_commands(
            "2026-07-12T00:06:00Z",
            Duration::from_secs(300),
            Duration::from_secs(45),
            fixture.state.instance_id(),
            &[],
        )
        .await
        .unwrap();

    assert_eq!(failed, 1);
    let command = fixture.command(prepared.command_id).await;
    assert_eq!(command.status, CommandStatus::Failed);
    let persisted: crate::repositories::FirmwarePersistedResult =
        serde_json::from_str(command.result_json.as_deref().unwrap()).unwrap();
    assert_eq!(
        persisted.phase,
        crate::repositories::FirmwarePersistedPhase::OutcomeUnknown
    );
}

#[tokio::test]
async fn firmware_typed_completion_remains_owned_during_cleanup() {
    let mut fixture = FirmwareFixture::new("firmware-completion-cleanup-owner").await;
    let prepared = fixture
        .prepare(upgrade_metadata("completion-cleanup"))
        .await;
    let waiter = fixture
        .start_execute(
            &prepared.prepared_token,
            upgrade_command("completion-cleanup"),
        )
        .await;
    let mut pause = crate::grpc::firmware_completion_pause::install(prepared.command_id);
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let agent_id = fixture.agent_id;
    let token = fixture.token;
    let event = AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: uuid::Uuid::new_v4().to_string(),
        event: Some(control_result_event(
            prepared.command_id,
            &fixture.serial,
            firmware_command_result::Outcome::Acknowledgement(FirmwareAcknowledgement {
                command: "upgrade_confirm".to_owned(),
                sequence_id: "completion-cleanup".to_owned(),
                result: Some("success".to_owned()),
                error_code: Some(0),
                reason: None,
                message: None,
            }),
        )),
    };
    let completion =
        tokio::spawn(async move { handle_event(&state, tenant_id, agent_id, token, event).await });
    pause.wait_until_reached().await;

    let pending = fixture.state.sessions().pending_live_command_ids().await;
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
    assert_eq!(failed, 0);
    assert_eq!(
        fixture.command(prepared.command_id).await.status,
        CommandStatus::Acknowledged
    );

    pause.resume();
    completion.await.unwrap().unwrap();
    assert_eq!(
        waiter.await.unwrap().phase,
        FirmwareExecutePhase::Acknowledged
    );
    assert_eq!(
        fixture.command(prepared.command_id).await.status,
        CommandStatus::Succeeded
    );
}

#[tokio::test]
async fn firmware_typed_rejection_racing_sibling_cleanup_resolves_only_durable_phase() {
    let mut fixture = FirmwareFixture::new_file("firmware-typed-terminal-sibling-race").await;
    let prepared = fixture
        .prepare(upgrade_metadata("typed-terminal-sibling-race"))
        .await;
    let waiter = fixture
        .start_execute(
            &prepared.prepared_token,
            upgrade_command("typed-terminal-sibling-race"),
        )
        .await;
    fixture
        .set_command_updated_at(prepared.command_id, "2026-07-12T00:00:00Z")
        .await;
    let mut pause = crate::grpc::firmware_completion_pause::install(prepared.command_id);
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let agent_id = fixture.agent_id;
    let token = fixture.token;
    let serial = fixture.serial.clone();
    let completion = tokio::spawn(async move {
        handle_event(
            &state,
            tenant_id,
            agent_id,
            token,
            AgentEvent {
                tenant_id: tenant_id.to_string(),
                agent_id: agent_id.to_string(),
                event_id: uuid::Uuid::new_v4().to_string(),
                event: Some(control_result_event(
                    prepared.command_id,
                    &serial,
                    firmware_command_result::Outcome::Acknowledgement(FirmwareAcknowledgement {
                        command: "upgrade_confirm".to_owned(),
                        sequence_id: "typed-terminal-sibling-race".to_owned(),
                        result: Some("fail".to_owned()),
                        error_code: Some(9),
                        reason: Some("printer rejected".to_owned()),
                        message: None,
                    }),
                )),
            },
        )
        .await
    });
    pause.wait_until_reached().await;

    let sibling = fixture.state.sibling_for_tests();
    let replacement = SessionToken::new();
    sibling
        .agents()
        .claim_online_session(
            fixture.tenant_id,
            fixture.agent_id,
            &replacement.persisted_id(),
            "typed-terminal-sibling-race",
            "2026-07-12T00:06:00Z",
        )
        .await
        .unwrap();
    let failed = sibling
        .commands()
        .fail_stale_unowned_live_commands(
            "2026-07-12T00:06:00Z",
            Duration::from_secs(300),
            Duration::from_secs(45),
            sibling.instance_id(),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(failed, 1);

    pause.resume();
    let _ = completion.await.unwrap();
    let result = waiter.await.unwrap();
    assert_eq!(result.phase, FirmwareExecutePhase::OutcomeUnknown);
    assert!(result.outcome.is_none());
    let durable = fixture.command(prepared.command_id).await;
    let persisted: crate::repositories::FirmwarePersistedResult =
        serde_json::from_str(durable.result_json.as_deref().unwrap()).unwrap();
    assert_eq!(
        persisted.phase,
        crate::repositories::FirmwarePersistedPhase::OutcomeUnknown
    );
}
