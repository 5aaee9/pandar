use super::fixture::FirmwareFixture;
use super::support::*;
use super::*;

#[tokio::test]
async fn firmware_command_durable_conversion_rejects_both_live_only_kinds() {
    let fixture = FirmwareFixture::new("firmware-command-conversion").await;
    for kind in ["firmware_refresh", "firmware_control"] {
        let id = CommandId::new();
        fixture
            .insert_raw_command(id, kind, CommandStatus::Sent)
            .await;
        let command = fixture.command(id).await;
        let error = crate::grpc::commands::hub_command_from_record(command).unwrap_err();
        assert_eq!(error.code(), Code::FailedPrecondition);
    }
}

#[tokio::test]
async fn firmware_command_prepare_rejection_and_closed_or_full_channel_are_pre_publish() {
    let mut fixture = FirmwareFixture::new("firmware-command-prepare-reject").await;
    let state = fixture.state.clone();
    let printer_id = fixture.printer_id.clone();
    let tenant_id = fixture.tenant_id;
    let prepare = tokio::spawn(async move {
        state
            .prepare_control(
                tenant_id,
                &printer_id,
                upgrade_metadata("reject"),
                audit_actor(),
            )
            .await
    });
    let outbound = fixture.next_command().await;
    let command_id = CommandId::parse(&outbound.command_id).unwrap();
    fixture
        .event(agent_event::Event::CommandAck(CommandAck {
            command_id: command_id.to_string(),
            accepted: false,
            error: "printer reservation busy".to_owned(),
        }))
        .await;
    assert!(matches!(
        prepare.await.unwrap(),
        Err(FirmwareServiceError::CommandFailed { .. })
    ));
    let command = fixture.command(command_id).await;
    assert_eq!(command.status, CommandStatus::Failed);
    assert!(command.error.unwrap().contains("reservation busy"));

    fixture.close_command_channel();
    let before = fixture.state.commands().count().await.unwrap();
    let error = fixture
        .state
        .prepare_control(
            fixture.tenant_id,
            &fixture.printer_id,
            upgrade_metadata("closed"),
            audit_actor(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, FirmwareServiceError::CommandFailed { .. }));
    assert_eq!(fixture.state.commands().count().await.unwrap(), before + 1);
    let latest = fixture.latest_command().await;
    assert_eq!(latest.status, CommandStatus::Failed);
    assert!(latest.result_json.unwrap().contains("pre_publish_failure"));

    let full = FirmwareFixture::new("firmware-command-prepare-full").await;
    let session = full.state.sessions().get(full.agent_id).await.unwrap();
    for index in 0..8 {
        session
            .command_sender
            .try_send(Ok(pandar_protocol::agent::v1::HubCommand {
                command_id: format!("filler-{index}"),
                command: None,
            }))
            .unwrap();
    }
    let error = full
        .state
        .prepare_control(
            full.tenant_id,
            &full.printer_id,
            upgrade_metadata("full"),
            audit_actor(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, FirmwareServiceError::CommandFailed { .. }));
    assert_eq!(full.latest_command().await.status, CommandStatus::Failed);
}

#[tokio::test]
async fn firmware_command_execute_sent_precedes_dispatch_and_publish_changes_failure_phase() {
    let mut fixture = FirmwareFixture::new("firmware-command-phases").await;
    let first = fixture.prepare(upgrade_metadata("before-publish")).await;
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let token = first.prepared_token.clone();
    let execute = tokio::spawn(async move {
        state
            .execute_control(tenant_id, &token, upgrade_command("before-publish"))
            .await
            .unwrap()
    });
    let _ = fixture.next_command().await;
    assert_eq!(
        fixture.command(first.command_id).await.status,
        CommandStatus::Acknowledged
    );
    fixture
        .event(agent_event::Event::CommandResult(CommandResult {
            command_id: first.command_id.to_string(),
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

    let second = fixture.prepare(upgrade_metadata("after-publish")).await;
    let state = fixture.state.clone();
    let tenant_id = fixture.tenant_id;
    let token = second.prepared_token.clone();
    let execute = tokio::spawn(async move {
        state
            .execute_control(tenant_id, &token, upgrade_command("after-publish"))
            .await
            .unwrap()
    });
    let _ = fixture.next_command().await;
    fixture
        .event(agent_event::Event::FirmwarePublished(FirmwarePublished {
            command_id: second.command_id.to_string(),
            serial: fixture.serial.clone(),
            generation: GENERATION,
        }))
        .await;
    fixture
        .event(agent_event::Event::CommandResult(CommandResult {
            command_id: second.command_id.to_string(),
            success: false,
            error: "acknowledgement wait failed".to_owned(),
            result_json: String::new(),
            firmware_result: None,
        }))
        .await;
    assert_eq!(
        execute.await.unwrap().phase,
        FirmwareExecutePhase::OutcomeUnknown
    );
}

#[tokio::test]
async fn firmware_command_rejection_and_published_without_ack_are_typed_terminal_phases() {
    let mut fixture = FirmwareFixture::new("firmware-command-terminals").await;
    let rejected = fixture.prepare(upgrade_metadata("rejected")).await;
    let rejected_waiter = fixture
        .start_execute(&rejected.prepared_token, upgrade_command("rejected"))
        .await;
    fixture
        .event(control_result_event(
            rejected.command_id,
            &fixture.serial,
            firmware_command_result::Outcome::Acknowledgement(FirmwareAcknowledgement {
                command: "upgrade_confirm".to_owned(),
                sequence_id: "rejected".to_owned(),
                result: Some("fail".to_owned()),
                error_code: Some(17),
                reason: Some("printer rejected".to_owned()),
                message: None,
            }),
        ))
        .await;
    assert_eq!(
        rejected_waiter.await.unwrap().phase,
        FirmwareExecutePhase::Rejected
    );
    assert_eq!(
        fixture.command(rejected.command_id).await.status,
        CommandStatus::Failed
    );

    let unknown = fixture.prepare(upgrade_metadata("no-ack")).await;
    let unknown_waiter = fixture
        .start_execute(&unknown.prepared_token, upgrade_command("no-ack"))
        .await;
    fixture
        .event(control_result_event(
            unknown.command_id,
            &fixture.serial,
            firmware_command_result::Outcome::PublishedWithoutAcknowledgement(
                pandar_protocol::agent::v1::PublishedWithoutAcknowledgement {},
            ),
        ))
        .await;
    let result = unknown_waiter.await.unwrap();
    assert_eq!(result.phase, FirmwareExecutePhase::OutcomeUnknown);
    assert!(matches!(
        result.outcome,
        Some(FirmwareTerminalOutcome::PublishedWithoutAcknowledgement)
    ));
    assert!(!result.error.unwrap().to_ascii_lowercase().contains("retry"));
}

#[tokio::test]
async fn firmware_lifecycle_wrong_typed_result_after_execute_is_outcome_unknown() {
    let mut fixture = FirmwareFixture::new("firmware-lifecycle-wrong-result").await;
    let prepared = fixture.prepare(upgrade_metadata("wrong-result")).await;
    let execute = fixture
        .start_execute(&prepared.prepared_token, upgrade_command("wrong-result"))
        .await;
    fixture
        .event(agent_event::Event::FirmwarePublished(FirmwarePublished {
            command_id: prepared.command_id.to_string(),
            serial: fixture.serial.clone(),
            generation: GENERATION,
        }))
        .await;

    let error = handle_event(
        &fixture.state,
        fixture.tenant_id,
        fixture.agent_id,
        fixture.token,
        AgentEvent {
            tenant_id: fixture.tenant_id.to_string(),
            agent_id: fixture.agent_id.to_string(),
            event_id: uuid::Uuid::new_v4().to_string(),
            event: Some(control_result_event(
                prepared.command_id,
                &fixture.serial,
                firmware_command_result::Outcome::RefreshedModules(FirmwareRefreshedModules {
                    modules: Vec::new(),
                    module_revision: 1,
                }),
            )),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code(), Code::InvalidArgument);

    let result = execute.await.unwrap();
    assert_eq!(result.phase, FirmwareExecutePhase::OutcomeUnknown);
    let persisted = fixture.command(prepared.command_id).await;
    assert_eq!(persisted.status, CommandStatus::Failed);
    assert!(persisted.result_json.unwrap().contains("outcome_unknown"));
}

#[tokio::test]
async fn firmware_command_metadata_mismatch_consumes_prepared_token_once() {
    let mut fixture = FirmwareFixture::new("firmware-command-token-mismatch").await;
    let prepared = fixture.prepare(upgrade_metadata("expected")).await;
    let mismatch = fixture
        .state
        .execute_control(
            fixture.tenant_id,
            &prepared.prepared_token,
            upgrade_command("different"),
        )
        .await
        .unwrap_err();
    assert!(matches!(mismatch, FirmwareServiceError::MetadataMismatch));
    let second = tokio::time::timeout(
        Duration::from_millis(50),
        fixture.state.execute_control(
            fixture.tenant_id,
            &prepared.prepared_token,
            upgrade_command("expected"),
        ),
    )
    .await;
    assert!(matches!(
        second,
        Ok(Err(FirmwareServiceError::InvalidPreparedToken))
    ));
    assert_eq!(
        fixture.command(prepared.command_id).await.status,
        CommandStatus::Failed
    );
}
