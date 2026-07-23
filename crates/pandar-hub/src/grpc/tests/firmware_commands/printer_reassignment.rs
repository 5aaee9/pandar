use super::fixture::FirmwareFixture;
use super::support::*;
use super::*;
use crate::repositories::{FirmwarePersistedPhase, FirmwarePersistedResult, PrinterSnapshotUpsert};

#[tokio::test]
async fn firmware_prepare_fails_closed_when_printer_is_reassigned_before_dispatch() {
    let mut fixture = FirmwareFixture::new("firmware-printer-reassignment-prepare").await;
    let mut pause =
        crate::firmware_control::dispatch_ownership_pause::install("prepare", &fixture.printer_id);
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let printer_id = fixture.printer_id.clone();
    let request = tokio::spawn(async move {
        state
            .prepare_control(
                tenant_id,
                &printer_id,
                upgrade_metadata("printer-reassignment-prepare"),
                audit_actor(),
            )
            .await
    });

    pause.wait_until_reached().await;
    let command = fixture.latest_command().await;
    let mut reassigned = reassign_to_new_agent(&fixture).await;
    pause.resume();

    let error = tokio::time::timeout(Duration::from_secs(2), request)
        .await
        .expect("prepare must fail after printer reassignment")
        .unwrap()
        .unwrap_err();
    assert!(matches!(error, FirmwareServiceError::Unavailable));
    assert_no_dispatch(&mut fixture.command_receiver, &mut reassigned).await;
    assert_pre_publish_terminal(&fixture, command.id).await;
    assert!(
        !fixture
            .state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&command.id)
    );
}

#[tokio::test]
async fn firmware_refresh_fails_closed_when_printer_is_reassigned_before_dispatch() {
    let mut fixture = FirmwareFixture::new("firmware-printer-reassignment-refresh").await;
    let mut pause =
        crate::firmware_control::dispatch_ownership_pause::install("refresh", &fixture.printer_id);
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let printer_id = fixture.printer_id.clone();
    let request = tokio::spawn(async move {
        state
            .refresh_version(
                tenant_id,
                &printer_id,
                "printer-reassignment-refresh".to_owned(),
                audit_actor(),
            )
            .await
    });

    pause.wait_until_reached().await;
    let command = fixture.latest_command().await;
    let mut reassigned = reassign_to_new_agent(&fixture).await;
    pause.resume();

    let error = tokio::time::timeout(Duration::from_secs(2), request)
        .await
        .expect("refresh must fail after printer reassignment")
        .unwrap()
        .unwrap_err();
    assert!(matches!(error, FirmwareServiceError::Unavailable));
    assert_no_dispatch(&mut fixture.command_receiver, &mut reassigned).await;
    assert_pre_publish_terminal(&fixture, command.id).await;
    assert!(
        !fixture
            .state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&command.id)
    );
}

#[tokio::test]
async fn firmware_execute_fails_closed_when_printer_is_reassigned_before_dispatch() {
    let mut fixture = FirmwareFixture::new("firmware-printer-reassignment-execute").await;
    let prepared = fixture.prepare(start_metadata()).await;
    let command_before = fixture.command(prepared.command_id).await;
    let identity = fixture
        .state
        .sessions()
        .firmware_token_locator(&prepared.prepared_token)
        .unwrap();
    let mut pause =
        crate::firmware_control::dispatch_ownership_pause::install("execute", &fixture.printer_id);
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let prepared_token = prepared.prepared_token.clone();
    let request = tokio::spawn(async move {
        state
            .execute_control(tenant_id, &prepared_token, start_command())
            .await
    });

    pause.wait_until_reached().await;
    let mut reassigned = reassign_to_new_agent(&fixture).await;
    pause.resume();

    let error = tokio::time::timeout(Duration::from_secs(2), request)
        .await
        .expect("execute must fail after printer reassignment")
        .unwrap()
        .unwrap_err();
    assert!(matches!(error, FirmwareServiceError::Unavailable));
    assert!(!format!("{error:#}").contains(URL_SENTINEL));
    assert_no_dispatch(&mut fixture.command_receiver, &mut reassigned).await;
    assert_eq!(fixture.command(prepared.command_id).await, command_before);
    assert!(
        fixture
            .state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&prepared.command_id)
    );
    assert_eq!(
        fixture
            .state
            .sessions()
            .firmware_token_locator(&prepared.prepared_token),
        Some(identity.clone())
    );
    assert_eq!(
        fixture
            .state
            .sessions()
            .retained_firmware_redaction_url_count(&identity),
        0
    );

    reassign_to_original_agent(&fixture).await;
    let waiter = fixture
        .start_execute(&prepared.prepared_token, start_command())
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
    assert_pre_publish_terminal(&fixture, prepared.command_id).await;
}

#[tokio::test]
async fn firmware_execute_fails_pre_publish_when_reassigned_after_durable_transition() {
    let mut fixture = FirmwareFixture::new("firmware-printer-reassignment-after-durable").await;
    let prepared = fixture.prepare(start_metadata()).await;
    let mut pause = crate::firmware_control::dispatch_ownership_pause::install(
        "execute-durable",
        &fixture.printer_id,
    );
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let prepared_token = prepared.prepared_token.clone();
    let request = tokio::spawn(async move {
        state
            .execute_control(tenant_id, &prepared_token, start_command())
            .await
    });

    pause.wait_until_reached().await;
    assert_eq!(
        fixture.command(prepared.command_id).await.status,
        CommandStatus::Acknowledged
    );
    let mut reassigned = reassign_to_new_agent(&fixture).await;
    pause.resume();

    let result = tokio::time::timeout(Duration::from_secs(2), request)
        .await
        .expect("execute must finish after post-transition printer reassignment")
        .unwrap()
        .unwrap();
    assert_eq!(result.phase, FirmwareExecutePhase::PrePublishFailure);
    assert_no_dispatch(&mut fixture.command_receiver, &mut reassigned).await;
    assert_pre_publish_terminal(&fixture, prepared.command_id).await;
    assert!(
        !fixture
            .state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&prepared.command_id)
    );
    assert!(
        fixture
            .state
            .sessions()
            .firmware_token_locator(&prepared.prepared_token)
            .is_none()
    );
}

async fn reassign_to_new_agent(
    fixture: &FirmwareFixture,
) -> mpsc::Receiver<Result<crate::protocol::agent::v1::HubCommand, Status>> {
    let agent = fixture
        .state
        .agents()
        .create(fixture.tenant_id, "reassigned-firmware-agent")
        .await
        .unwrap();
    let token = SessionToken::new();
    let (wake_sender, _) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);
    let (command_sender, command_receiver) = mpsc::channel(8);
    {
        let _lease = fixture
            .state
            .sessions()
            .transition_lease_for_session(agent.id, token)
            .await;
        fixture
            .state
            .agents()
            .claim_online_session(
                fixture.tenant_id,
                agent.id,
                &token.persisted_id(),
                "reassigned-firmware-test",
                "2026-07-13T00:00:00Z",
            )
            .await
            .unwrap();
        fixture
            .state
            .sessions()
            .register(AgentSession {
                token,
                tenant_id: fixture.tenant_id,
                agent_id: agent.id,
                name: "reassigned firmware agent".to_owned(),
                version: "test".to_owned(),
                connected_at: "2026-07-13T00:00:00Z".to_owned(),
                last_heartbeat_at: "2026-07-13T00:00:00Z".to_owned(),
                wake_sender,
                close_sender,
                command_sender,
                capabilities: HashSet::from([AgentCapability::FirmwareControl]),
                pending_live_commands: empty_pending_live_commands(),
                live_command_transition: Arc::new(tokio::sync::Mutex::new(())),
            })
            .await;
        let reassigned = fixture
            .state
            .printers()
            .upsert_snapshot_if_current(
                fixture.tenant_id,
                agent.id,
                &token.persisted_id(),
                reassignment_snapshot(fixture),
            )
            .await
            .unwrap();
        assert_eq!(reassigned.id, fixture.printer_id);
        assert_eq!(reassigned.agent_id, agent.id);
    }
    command_receiver
}

async fn reassign_to_original_agent(fixture: &FirmwareFixture) {
    let reassigned = fixture
        .state
        .printers()
        .upsert_snapshot_if_current(
            fixture.tenant_id,
            fixture.agent_id,
            &fixture.token.persisted_id(),
            reassignment_snapshot(fixture),
        )
        .await
        .unwrap();
    assert_eq!(reassigned.id, fixture.printer_id);
    assert_eq!(reassigned.agent_id, fixture.agent_id);
}

fn reassignment_snapshot(fixture: &FirmwareFixture) -> PrinterSnapshotUpsert {
    PrinterSnapshotUpsert {
        serial_number: fixture.serial.clone(),
        host: None,
        access_code: None,
        name: "reassigned printer".to_owned(),
        model: None,
        status: Some("idle".to_owned()),
        observed_at: "2026-07-13T00:00:00Z".to_owned(),
        nozzle_temperatures: Vec::new(),
        active_nozzle: None,
        bed_temperature_celsius: None,
        bed_target_temperature_celsius: None,
        chamber_temperature_celsius: None,
        chamber_target_temperature_celsius: None,
        chamber_light_on: None,
        connection_authoritative: false,
        telemetry_authoritative: true,
    }
}

async fn assert_no_dispatch(
    original: &mut mpsc::Receiver<Result<crate::protocol::agent::v1::HubCommand, Status>>,
    reassigned: &mut mpsc::Receiver<Result<crate::protocol::agent::v1::HubCommand, Status>>,
) {
    tokio::task::yield_now().await;
    assert!(original.try_recv().is_err());
    assert!(reassigned.try_recv().is_err());
}

async fn assert_pre_publish_terminal(fixture: &FirmwareFixture, command_id: CommandId) {
    let command = fixture.command(command_id).await;
    assert_eq!(command.status, CommandStatus::Failed);
    let result: FirmwarePersistedResult =
        serde_json::from_str(command.result_json.as_deref().unwrap()).unwrap();
    assert_eq!(result.phase, FirmwarePersistedPhase::PrePublishFailure);
    let readback = serde_json::to_string(&command).unwrap();
    assert!(!readback.contains(URL_SENTINEL));
}
