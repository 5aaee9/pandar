use super::*;
use crate::repositories::PrinterSnapshotUpsert;

#[tokio::test]
async fn completed_start_url_redacts_next_generation_module_and_status_snapshots() {
    let mut fixture = FirmwareFixture::new("firmware-redaction-next-generation").await;
    complete_start(&mut fixture).await;

    let generation = GENERATION + 1;
    fixture
        .event(agent_event::Event::PrinterFirmwareInvalidated(
            PrinterFirmwareInvalidated {
                serial: fixture.serial.clone(),
                generation,
            },
        ))
        .await;
    persist_leaking_snapshots(&fixture, fixture.agent_id, fixture.token, generation).await;

    assert_persisted_firmware_is_redacted(&fixture).await;
}

#[tokio::test]
async fn completed_start_url_redacts_reassigned_agent_module_and_status_snapshots() {
    let mut fixture = FirmwareFixture::new("firmware-redaction-reassigned-agent").await;
    complete_start(&mut fixture).await;

    let lease = fixture
        .state
        .sessions()
        .transition_lease_for_session(fixture.agent_id, fixture.token)
        .await;
    let cancelled = fixture
        .state
        .sessions()
        .cancel_firmware_session_under_transition(fixture.agent_id, fixture.token);
    drop(lease);
    crate::firmware_control::finish_cancelled_commands(
        &fixture.state,
        cancelled,
        "test original firmware session cancellation",
    )
    .await;

    let (agent_id, token, _command_receiver) = reassign_to_new_agent(&fixture).await;
    persist_leaking_snapshots(&fixture, agent_id, token, GENERATION + 1).await;

    assert_persisted_firmware_is_redacted(&fixture).await;
}

async fn complete_start(fixture: &mut FirmwareFixture) {
    let prepared = fixture.prepare(start_metadata()).await;
    let waiter = fixture
        .start_execute(&prepared.prepared_token, start_command())
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
    assert_eq!(
        waiter.await.unwrap().phase,
        FirmwareExecutePhase::Acknowledged
    );
}

async fn reassign_to_new_agent(
    fixture: &FirmwareFixture,
) -> (
    AgentId,
    SessionToken,
    mpsc::Receiver<Result<crate::protocol::agent::v1::HubCommand, tonic::Status>>,
) {
    let agent = fixture
        .state
        .agents()
        .create(fixture.tenant_id, "firmware-redaction-reassigned-agent")
        .await
        .unwrap();
    let token = SessionToken::new();
    let (wake_sender, _) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);
    let (command_sender, command_receiver) = mpsc::channel(1);
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
            "firmware-redaction-reassigned-agent",
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
            name: "firmware redaction reassigned agent".to_owned(),
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
    fixture
        .state
        .printers()
        .upsert_snapshot_if_current(
            fixture.tenant_id,
            agent.id,
            &token.persisted_id(),
            PrinterSnapshotUpsert {
                serial_number: fixture.serial.clone(),
                host: None,
                access_code: None,
                name: "firmware redaction reassigned printer".to_owned(),
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
                nozzle_system: None,
                connection_authoritative: false,
                telemetry_authoritative: true,
            },
        )
        .await
        .unwrap();
    fixture
        .state
        .printers()
        .establish_generation_if_current(
            fixture.tenant_id,
            agent.id,
            &token.persisted_id(),
            &fixture.serial,
            GENERATION + 1,
        )
        .await
        .unwrap();
    (agent.id, token, command_receiver)
}

async fn persist_leaking_snapshots(
    fixture: &FirmwareFixture,
    agent_id: AgentId,
    token: SessionToken,
    generation: u64,
) {
    event_as(
        fixture,
        agent_id,
        token,
        agent_event::Event::PrinterFirmwareModulesSnapshot(PrinterFirmwareModulesSnapshot {
            serial: fixture.serial.clone(),
            generation,
            module_revision: 1,
            modules: vec![leaking_module()],
        }),
    )
    .await;
    event_as(
        fixture,
        agent_id,
        token,
        agent_event::Event::PrinterFirmwareStatusSnapshot(PrinterFirmwareStatusSnapshot {
            serial: fixture.serial.clone(),
            generation,
            status_revision: 1,
            upgrade_state: Some(leaking_upgrade_state()),
            cfg: Some(leaking_value("retained-cfg")),
        }),
    )
    .await;
}

async fn event_as(
    fixture: &FirmwareFixture,
    agent_id: AgentId,
    token: SessionToken,
    event: agent_event::Event,
) {
    handle_event(
        &fixture.state,
        fixture.tenant_id,
        agent_id,
        token,
        AgentEvent {
            tenant_id: fixture.tenant_id.to_string(),
            agent_id: agent_id.to_string(),
            event_id: uuid::Uuid::new_v4().to_string(),
            event: Some(event),
        },
    )
    .await
    .unwrap();
}

async fn assert_persisted_firmware_is_redacted(fixture: &FirmwareFixture) {
    let readback = serde_json::to_string(
        &fixture
            .state
            .printers()
            .get_with_live_status_for_tenant(fixture.tenant_id, &fixture.printer_id)
            .await
            .unwrap()
            .unwrap()
            .firmware,
    )
    .unwrap();
    for forbidden in [
        URL_SENTINEL,
        "FIRMWARE-URL-SENTINEL",
        "/main.bin",
        "user",
        "secret",
    ] {
        assert!(
            !readback.contains(forbidden),
            "retained Start URL leaked {forbidden}: {readback}"
        );
    }
    assert!(readback.contains("[redacted]"));
}
