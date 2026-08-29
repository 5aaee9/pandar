use super::*;

#[tokio::test]
async fn tenant_printer_control_accepts_semantic_recovery_and_dispatches_server_owned_payload() {
    let mut fixture = RecoveryFixture::new(
        "tenant-native-success",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;

    let (status, body) = fixture.request("resume", ERROR_GENERATION).await;

    assert_eq!(status, StatusCode::OK);
    let response = decode::<CommandResponse>(body);
    assert_eq!(response.status, "sent");
    let command_id = CommandId::parse(&response.id).unwrap();
    let persisted = fixture
        .state
        .commands()
        .get_for_tenant(fixture.tenant_id, command_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.status, CommandStatus::Sent);
    assert_eq!(
        serde_json::from_str::<PrinterOperationPayload>(&persisted.payload_json)
            .unwrap()
            .operation,
        PrinterOperationKind::HandlePrintError {
            error_action: PrintErrorAction::Resume,
            print_error: BUILD_PLATE_MISMATCH,
            printer_job_id: "job-7".to_owned(),
            sequence_id: 0,
        }
    );

    let emitted = fixture.command_receiver.recv().await.unwrap().unwrap();
    let Some(hub_command::Command::PrinterOperation(operation)) = emitted.command else {
        panic!("expected printer operation command");
    };
    let Some(printer_operation::Operation::HandlePrintError(operation)) = operation.operation
    else {
        panic!("expected handle print error operation");
    };
    assert_eq!(operation.error_action, ProtoPrintErrorAction::Resume as i32);
    assert_eq!(operation.print_error, BUILD_PLATE_MISMATCH);
    assert_eq!(operation.printer_job_id, "job-7");
    assert_eq!(operation.sequence_id, 0);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), fixture.wake_receiver.recv())
            .await
            .is_err(),
        "Web recovery must never wake the durable command pump"
    );
    assert!(
        fixture
            .state
            .commands()
            .next_queued_for_agent(fixture.tenant_id, fixture.agent_id)
            .await
            .unwrap()
            .is_none()
    );
    let events = fixture
        .state
        .audit_events()
        .list_for_tenant(fixture.tenant_id)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].actor_type, "tenant_token");
    assert_eq!(events[0].action, "printer.dispatch_control");
    assert_eq!(events[0].target_type, "printer");
    assert_eq!(
        events[0].target_id.as_deref(),
        Some(fixture.printer_id.as_str())
    );
    let metadata: WebPrintErrorAuditMetadata =
        serde_json::from_str(&events[0].metadata_json).unwrap();
    assert_eq!(metadata.agent_id, fixture.agent_id.to_string());
    assert_eq!(metadata.serial_number, "20P123456789");
    assert_eq!(metadata.action, "handle_print_error");
    assert_eq!(metadata.error_action, PrintErrorAction::Resume);
    assert_eq!(metadata.print_error, BUILD_PLATE_MISMATCH);
    assert_eq!(metadata.printer_job_id, "job-7");
    assert_eq!(metadata.sequence_id, 0);
    assert!(!metadata.tenant_token_id.is_empty());
    assert_eq!(metadata.tenant_token_scopes, ["*"]);
}

#[tokio::test]
async fn tenant_recovery_requires_operator_and_hides_cross_tenant_printers() {
    let fixture = RecoveryFixture::new(
        "tenant-native-auth",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    let viewer = auth_token_for_role(
        &fixture.state,
        &fixture.tenant_id.to_string(),
        UserRole::Viewer,
        "tenant-native-viewer",
    )
    .await;
    let (status, body) = request_as(
        fixture.app.clone(),
        Method::POST,
        &fixture.uri,
        Some(recovery_body("resume", ERROR_GENERATION)),
        &viewer,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");

    let other = fixture
        .state
        .tenants()
        .create("tenant-native-other", "Other")
        .await
        .unwrap();
    let other_token = auth_token_for_role(
        &fixture.state,
        &other.id.to_string(),
        UserRole::Operator,
        "tenant-native-other-token",
    )
    .await;
    let (status, body) = request_as(
        fixture.app.clone(),
        Method::POST,
        &format!(
            "/api/v1/tenants/{}/printers/{}/controls",
            other.id, fixture.printer_id
        ),
        Some(recovery_body("resume", ERROR_GENERATION)),
        &other_token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(decode::<ErrorResponse>(body).error, "printer_not_found");

    let missing_printer_id = uuid::Uuid::new_v4();
    let (status, body) = request_as(
        fixture.app.clone(),
        Method::POST,
        &format!(
            "/api/v1/tenants/{}/printers/{missing_printer_id}/controls",
            fixture.tenant_id
        ),
        Some(recovery_body("resume", ERROR_GENERATION)),
        &fixture.token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(decode::<ErrorResponse>(body).error, "printer_not_found");
}

#[tokio::test]
async fn tenant_recovery_fails_closed_for_old_agent_capability() {
    let fixture = RecoveryFixture::new(
        "tenant-native-old-agent",
        "20P123456789",
        [AgentCapability::HandlePrintError],
    )
    .await;

    let (status, body) = fixture.request("resume", ERROR_GENERATION).await;

    assert_unavailable(status, body);
    assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
}
