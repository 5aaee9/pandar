use std::{sync::Arc, time::Duration};

use tokio::sync::Barrier;

use super::*;

#[tokio::test]
async fn sqlite_web_recovery_blocks_plugin_until_terminal_then_allows_retry() {
    let mut fixture = RecoveryFixture::new_file(
        "tenant-native-web-first-plugin",
        "20P123456789",
        [
            AgentCapability::HandlePrintError,
            AgentCapability::HandlePrintErrorSequenceZeroPubackOnly,
        ],
    )
    .await;

    let (status, body) = fixture.request("resume", ERROR_GENERATION).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let web = decode::<CommandResponse>(body);
    assert_eq!(web.status, "sent");
    assert_persisted_and_emitted(
        &mut fixture,
        &web.id,
        PrinterOperationKind::HandlePrintError {
            error_action: PrintErrorAction::Resume,
            print_error: BUILD_PLATE_MISMATCH,
            printer_job_id: "job-7".to_owned(),
            sequence_id: 0,
        },
    )
    .await;

    let (status, body) = fixture.plugin_request("stop").await;
    assert_unavailable(status, body);
    assert_eq!(fixture.state.commands().count().await.unwrap(), 1);
    assert_no_second_emission(&mut fixture, "blocked plugin recovery").await;
    assert_live_only(&mut fixture).await;

    fixture
        .state
        .commands()
        .mark_succeeded(
            CommandId::parse(&web.id).unwrap(),
            fixture.tenant_id,
            fixture.agent_id,
        )
        .await
        .unwrap();
    let (status, body) = fixture.plugin_request("stop").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let plugin = decode::<PluginOperationResponse>(body);
    assert_eq!(plugin.status, "sent");
    assert_persisted_and_emitted(
        &mut fixture,
        &plugin.command_id,
        PrinterOperationKind::HandlePrintError {
            error_action: PrintErrorAction::Stop,
            print_error: BUILD_PLATE_MISMATCH,
            printer_job_id: "job-7".to_owned(),
            sequence_id: 20_042,
        },
    )
    .await;
    assert_eq!(fixture.state.commands().count().await.unwrap(), 2);
}

#[tokio::test]
async fn sqlite_simultaneous_web_and_plugin_recovery_persists_and_emits_one_then_retries() {
    let mut fixture = RecoveryFixture::new_file(
        "tenant-native-mixed-race",
        "20P123456789",
        [
            AgentCapability::HandlePrintError,
            AgentCapability::HandlePrintErrorSequenceZeroPubackOnly,
        ],
    )
    .await;
    let barrier = Arc::new(Barrier::new(3));
    let web = tokio::spawn({
        let barrier = barrier.clone();
        let app = fixture.app.clone();
        let uri = fixture.uri.clone();
        let token = fixture.token.clone();
        async move {
            barrier.wait().await;
            request_as(
                app,
                Method::POST,
                &uri,
                Some(recovery_body("resume", ERROR_GENERATION)),
                &token,
            )
            .await
        }
    });
    let plugin = tokio::spawn({
        let barrier = barrier.clone();
        let app = fixture.app.clone();
        let uri = fixture.plugin_uri.clone();
        let token = fixture.plugin_token.clone();
        async move {
            barrier.wait().await;
            request_as(
                app,
                Method::POST,
                &uri,
                Some(plugin_recovery_body("stop")),
                &token,
            )
            .await
        }
    });
    barrier.wait().await;
    let (web, plugin) = tokio::join!(web, plugin);
    let web = web.unwrap();
    let plugin = plugin.unwrap();

    assert_eq!(
        [web.0, plugin.0]
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        [web.0, plugin.0]
            .iter()
            .filter(|status| **status == StatusCode::BAD_REQUEST)
            .count(),
        1
    );
    let web_won = web.0 == StatusCode::OK;
    let (winner_id, winner_operation) = if web_won {
        let response = decode::<CommandResponse>(web.1);
        assert_eq!(response.status, "sent");
        assert_unavailable(plugin.0, plugin.1);
        (
            response.id,
            PrinterOperationKind::HandlePrintError {
                error_action: PrintErrorAction::Resume,
                print_error: BUILD_PLATE_MISMATCH,
                printer_job_id: "job-7".to_owned(),
                sequence_id: 0,
            },
        )
    } else {
        assert_unavailable(web.0, web.1);
        let response = decode::<PluginOperationResponse>(plugin.1);
        assert_eq!(response.status, "sent");
        (
            response.command_id,
            PrinterOperationKind::HandlePrintError {
                error_action: PrintErrorAction::Stop,
                print_error: BUILD_PLATE_MISMATCH,
                printer_job_id: "job-7".to_owned(),
                sequence_id: 20_042,
            },
        )
    };
    assert_eq!(fixture.state.commands().count().await.unwrap(), 1);
    assert_persisted_and_emitted(&mut fixture, &winner_id, winner_operation).await;
    assert_no_second_emission(&mut fixture, "losing concurrent recovery").await;
    assert_live_only(&mut fixture).await;

    fixture
        .state
        .commands()
        .mark_succeeded(
            CommandId::parse(&winner_id).unwrap(),
            fixture.tenant_id,
            fixture.agent_id,
        )
        .await
        .unwrap();
    let (retry_id, retry_operation) = if web_won {
        let (status, body) = fixture.plugin_request("stop").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let response = decode::<PluginOperationResponse>(body);
        assert_eq!(response.status, "sent");
        (
            response.command_id,
            PrinterOperationKind::HandlePrintError {
                error_action: PrintErrorAction::Stop,
                print_error: BUILD_PLATE_MISMATCH,
                printer_job_id: "job-7".to_owned(),
                sequence_id: 20_042,
            },
        )
    } else {
        let (status, body) = fixture.request("resume", ERROR_GENERATION).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let response = decode::<CommandResponse>(body);
        assert_eq!(response.status, "sent");
        (
            response.id,
            PrinterOperationKind::HandlePrintError {
                error_action: PrintErrorAction::Resume,
                print_error: BUILD_PLATE_MISMATCH,
                printer_job_id: "job-7".to_owned(),
                sequence_id: 0,
            },
        )
    };
    assert_persisted_and_emitted(&mut fixture, &retry_id, retry_operation).await;
    assert_eq!(fixture.state.commands().count().await.unwrap(), 2);
}

async fn assert_persisted_and_emitted(
    fixture: &mut RecoveryFixture,
    command_id: &str,
    expected_operation: PrinterOperationKind,
) {
    let command_id = CommandId::parse(command_id).unwrap();
    let persisted = fixture
        .state
        .commands()
        .get_for_tenant(fixture.tenant_id, command_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.status, CommandStatus::Sent);
    assert_eq!(
        persisted.printer_id.as_deref(),
        Some(fixture.printer_id.as_str())
    );
    let payload: PrinterOperationPayload = serde_json::from_str(&persisted.payload_json).unwrap();
    assert_eq!(
        payload,
        PrinterOperationPayload {
            printer_id: fixture.printer_id.clone(),
            serial_number: "20P123456789".to_owned(),
            operation: expected_operation.clone(),
        }
    );

    let emitted = tokio::time::timeout(Duration::from_secs(1), fixture.command_receiver.recv())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(emitted.command_id, command_id.to_string());
    let Some(hub_command::Command::PrinterOperation(operation)) = emitted.command else {
        panic!("expected printer operation command");
    };
    assert_eq!(operation.serial_number, "20P123456789");
    let Some(printer_operation::Operation::HandlePrintError(operation)) = operation.operation
    else {
        panic!("expected handle print error operation");
    };
    let PrinterOperationKind::HandlePrintError {
        error_action,
        print_error,
        printer_job_id,
        sequence_id,
    } = expected_operation
    else {
        panic!("expected handle print error operation");
    };
    let error_action = match error_action {
        PrintErrorAction::Resume => ProtoPrintErrorAction::Resume,
        PrintErrorAction::Ignore => ProtoPrintErrorAction::Ignore,
        PrintErrorAction::Stop => ProtoPrintErrorAction::Stop,
    };
    assert_eq!(operation.error_action, error_action as i32);
    assert_eq!(operation.print_error, print_error);
    assert_eq!(operation.printer_job_id, printer_job_id);
    assert_eq!(operation.sequence_id, sequence_id);
}

async fn assert_no_second_emission(fixture: &mut RecoveryFixture, label: &str) {
    assert!(
        tokio::time::timeout(Duration::from_millis(50), fixture.command_receiver.recv())
            .await
            .is_err(),
        "{label} emitted a second operation"
    );
}

async fn assert_live_only(fixture: &mut RecoveryFixture) {
    assert!(
        fixture
            .state
            .commands()
            .next_queued_for_agent(fixture.tenant_id, fixture.agent_id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), fixture.wake_receiver.recv())
            .await
            .is_err(),
        "native recovery woke the durable command pump"
    );
}

#[derive(Debug, Deserialize)]
struct PluginOperationResponse {
    command_id: String,
    status: String,
}
