use super::fixture::FirmwareFixture;
use super::support::*;
use super::*;

#[tokio::test]
async fn firmware_lifecycle_negative_ack_remains_owned_during_terminal_persistence() {
    let mut fixture = FirmwareFixture::new("firmware-negative-ack-cleanup-owner").await;
    let prepared = fixture
        .prepare(upgrade_metadata("negative-ack-cleanup"))
        .await;
    let waiter = fixture
        .start_execute(
            &prepared.prepared_token,
            upgrade_command("negative-ack-cleanup"),
        )
        .await;
    fixture
        .set_command_updated_at(prepared.command_id, "2026-07-12T00:00:00Z")
        .await;
    let mut pause = crate::firmware_control::lifecycle_finish_pause::install(prepared.command_id);
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let agent_id = fixture.agent_id;
    let token = fixture.token;
    let event = AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: uuid::Uuid::new_v4().to_string(),
        event: Some(agent_event::Event::CommandAck(CommandAck {
            command_id: prepared.command_id.to_string(),
            accepted: false,
            error: "agent rejected before publish".to_owned(),
        })),
    };
    let finish =
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
    pause.resume();
    finish.await.unwrap().unwrap();
    assert_eq!(
        waiter.await.unwrap().phase,
        FirmwareExecutePhase::PrePublishFailure
    );
    let command = fixture.command(prepared.command_id).await;
    assert_eq!(command.status, CommandStatus::Failed);
    assert!(command.result_json.unwrap().contains("pre_publish_failure"));
}

#[tokio::test]
async fn firmware_lifecycle_negative_ack_racing_sibling_cleanup_resolves_only_durable_phase() {
    let mut fixture = FirmwareFixture::new_file("firmware-lifecycle-terminal-sibling-race").await;
    let prepared = fixture
        .prepare(upgrade_metadata("lifecycle-terminal-sibling-race"))
        .await;
    let waiter = fixture
        .start_execute(
            &prepared.prepared_token,
            upgrade_command("lifecycle-terminal-sibling-race"),
        )
        .await;
    fixture
        .set_command_updated_at(prepared.command_id, "2026-07-12T00:00:00Z")
        .await;
    let mut pause = crate::firmware_control::lifecycle_finish_pause::install(prepared.command_id);
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let agent_id = fixture.agent_id;
    let token = fixture.token;
    let rejection = tokio::spawn(async move {
        handle_event(
            &state,
            tenant_id,
            agent_id,
            token,
            AgentEvent {
                tenant_id: tenant_id.to_string(),
                agent_id: agent_id.to_string(),
                event_id: uuid::Uuid::new_v4().to_string(),
                event: Some(agent_event::Event::CommandAck(CommandAck {
                    command_id: prepared.command_id.to_string(),
                    accepted: false,
                    error: "agent rejected before publish".to_owned(),
                })),
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
            "lifecycle-terminal-sibling-race",
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
    rejection.await.unwrap().unwrap();
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

#[tokio::test]
async fn firmware_lifecycle_post_publish_replacement_cleanup_remains_owned_until_outcome_unknown() {
    let mut fixture = FirmwareFixture::new("firmware-published-cleanup-owner").await;
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
        .set_command_updated_at(prepared.command_id, "2026-07-12T00:00:00Z")
        .await;
    let mut pause = crate::firmware_control::lifecycle_finish_pause::install(prepared.command_id);
    let replaced = fixture.replace_session_without_cleanup().await;
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let agent_id = fixture.agent_id;
    let cleanup = tokio::spawn(async move {
        crate::sessions::live_commands::fail_pending_live_commands(
            &state,
            tenant_id,
            agent_id,
            replaced,
            "agent session replaced before printer operation completed",
        )
        .await;
    });
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
    pause.resume();
    cleanup.await.unwrap();
    let result = waiter.await.unwrap();
    assert_eq!(result.phase, FirmwareExecutePhase::OutcomeUnknown);
    let command = fixture.command(prepared.command_id).await;
    assert_eq!(command.status, CommandStatus::Failed);
    let readback = serde_json::to_string(&(result, command)).unwrap();
    assert!(readback.contains("outcome_unknown"));
    assert!(!readback.contains(URL_SENTINEL));
}
