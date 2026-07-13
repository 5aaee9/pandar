use super::fixture::FirmwareFixture;
use super::support::*;
use super::*;

#[tokio::test]
async fn sibling_cleanup_skips_firmware_command_owned_by_fresh_authoritative_session() {
    let mut fixture = FirmwareFixture::new_file("firmware-fresh-sibling-cleanup").await;
    let prepared = fixture
        .prepare(upgrade_metadata("fresh-sibling-cleanup"))
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

    let sibling = fixture.state.sibling_for_tests();
    assert!(
        sibling
            .sessions()
            .pending_live_command_ids()
            .await
            .is_empty()
    );
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

    assert_eq!(failed, 0);
    assert_eq!(
        fixture.command(prepared.command_id).await.status,
        CommandStatus::Sent
    );
}

#[tokio::test]
async fn sibling_cleanup_fails_firmware_command_owned_by_stale_authoritative_session() {
    let mut fixture = FirmwareFixture::new_file("firmware-stale-sibling-cleanup").await;
    let prepared = fixture
        .prepare(upgrade_metadata("stale-sibling-cleanup"))
        .await;
    fixture
        .set_command_updated_at(prepared.command_id, "2026-07-12T00:00:00Z")
        .await;

    let sibling = fixture.state.sibling_for_tests();
    assert!(
        sibling
            .sessions()
            .pending_live_command_ids()
            .await
            .is_empty()
    );
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
    let command = fixture.command(prepared.command_id).await;
    assert_eq!(command.status, CommandStatus::Failed);
    let result: crate::repositories::FirmwarePersistedResult =
        serde_json::from_str(command.result_json.as_deref().unwrap()).unwrap();
    assert_eq!(
        result.phase,
        crate::repositories::FirmwarePersistedPhase::PrePublishFailure
    );
}

#[tokio::test]
async fn sibling_cleanup_fails_firmware_command_owned_by_expired_exact_session() {
    let mut fixture = FirmwareFixture::new_file("firmware-expired-owner-sibling-cleanup").await;
    let prepared = fixture
        .prepare(upgrade_metadata("expired-owner-sibling-cleanup"))
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
            "2026-07-12T00:05:00Z",
        )
        .await
        .unwrap();

    let sibling = fixture.state.sibling_for_tests();
    assert!(
        sibling
            .sessions()
            .pending_live_command_ids()
            .await
            .is_empty()
    );
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
    let command = fixture.command(prepared.command_id).await;
    assert_eq!(command.status, CommandStatus::Failed);
    let result: crate::repositories::FirmwarePersistedResult =
        serde_json::from_str(command.result_json.as_deref().unwrap()).unwrap();
    assert_eq!(
        result.phase,
        crate::repositories::FirmwarePersistedPhase::PrePublishFailure
    );
}

#[tokio::test]
async fn restarted_sibling_cleanup_fails_firmware_command_from_replaced_session() {
    let mut fixture = FirmwareFixture::new_file("firmware-restarted-sibling-cleanup").await;
    let prepared = fixture
        .prepare(upgrade_metadata("restarted-sibling-cleanup"))
        .await;
    fixture
        .set_command_updated_at(prepared.command_id, "2026-07-12T00:00:00Z")
        .await;

    let sibling = fixture.state.sibling_for_tests();
    let replacement = SessionToken::new();
    assert_ne!(replacement.persisted_id(), fixture.token.persisted_id());
    sibling
        .agents()
        .claim_online_session(
            fixture.tenant_id,
            fixture.agent_id,
            &replacement.persisted_id(),
            "restarted-sibling-test",
            "2026-07-12T00:06:00Z",
        )
        .await
        .unwrap();
    assert!(
        sibling
            .sessions()
            .pending_live_command_ids()
            .await
            .is_empty()
    );

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
    let command = fixture.command(prepared.command_id).await;
    assert_eq!(command.status, CommandStatus::Failed);
    let result: crate::repositories::FirmwarePersistedResult =
        serde_json::from_str(command.result_json.as_deref().unwrap()).unwrap();
    assert_eq!(
        result.phase,
        crate::repositories::FirmwarePersistedPhase::PrePublishFailure
    );
}

#[tokio::test]
async fn firmware_refresh_waiter_cannot_observe_before_paused_cas_finishes() {
    let mut fixture = FirmwareFixture::new("firmware-refresh-cas-before-waiter").await;
    let state = fixture.state.clone();
    let printer_id = fixture.printer_id.clone();
    let tenant_id = fixture.tenant_id;
    let refresh = tokio::spawn(async move {
        state
            .refresh_version(
                tenant_id,
                &printer_id,
                "refresh-cas-before-waiter".to_owned(),
                audit_actor(),
            )
            .await
    });
    let outbound = fixture.next_command().await;
    let command_id = CommandId::parse(&outbound.command_id).unwrap();
    let mut completion_pause = crate::grpc::firmware_completion_pause::install(command_id);
    let state = fixture.state.clone();
    let agent_id = fixture.agent_id;
    let token = fixture.token;
    let serial = fixture.serial.clone();
    let event = AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: uuid::Uuid::new_v4().to_string(),
        event: Some(control_result_event(
            command_id,
            &serial,
            firmware_command_result::Outcome::RefreshedModules(FirmwareRefreshedModules {
                modules: vec![module_with_version("ota", "01.02.05")],
                module_revision: 5,
            }),
        )),
    };
    let completion =
        tokio::spawn(async move { handle_event(&state, tenant_id, agent_id, token, event).await });
    completion_pause.wait_until_reached().await;
    let mut cas_pause =
        crate::repositories::current_transaction_pause::install(&fixture.token.persisted_id());
    completion_pause.resume();
    cas_pause.wait_until_reached().await;

    assert!(!refresh.is_finished());
    assert!(!completion.is_finished());
    cas_pause.resume();
    completion.await.unwrap().unwrap();
    let result = refresh.await.unwrap().unwrap();
    assert_eq!(result.module_revision, 5);
    assert_eq!(
        result.modules[0].software_version.as_deref(),
        Some("01.02.05")
    );
}
