use pandar_core::{CommandId, CommandStatus};
use tokio_stream::StreamExt;
use tonic::Code;

use super::*;
use crate::{
    grpc::commands::hub_command_from_record,
    repositories::{DiagnosePrinterPayload, DiscoverPrintersPayload},
};

#[tokio::test]
async fn grpc_wrong_agent_ack_is_permission_denied() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let other = state.agents().create(tenant_id, "other").await.unwrap();
    let command_id = sent_command(&state, tenant_id, agent_id).await;

    let err = handle_ack(
        &state,
        tenant_id,
        other.id,
        CommandAck {
            command_id: command_id.to_string(),
            accepted: true,
            error: String::new(),
        },
    )
    .await
    .unwrap_err();

    assert_eq!(err.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn grpc_wrong_agent_ack_streams_permission_denied() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let other = state.agents().create(tenant_id, "other").await.unwrap();
    let command_id = sent_command(&state, tenant_id, other.id).await;
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    sender
        .send(Ok(ack_event(tenant_id, agent_id, command_id)))
        .await
        .unwrap();
    let err = stream.next().await.unwrap().unwrap_err();

    assert_eq!(err.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn grpc_unknown_command_ack_is_not_found() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;

    let err = handle_ack(
        &state,
        tenant_id,
        agent_id,
        CommandAck {
            command_id: CommandId::new().to_string(),
            accepted: true,
            error: String::new(),
        },
    )
    .await
    .unwrap_err();

    assert_eq!(err.code(), Code::NotFound);
}

#[tokio::test]
async fn grpc_live_stream_ack_and_result_update_command_ledger() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let command = state
        .sessions()
        .dispatch_refresh_printers(tenant_id, agent_id, state.commands())
        .await
        .unwrap();
    let _ = stream.next().await.unwrap().unwrap();

    sender
        .send(Ok(ack_event(tenant_id, agent_id, command.id)))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let err = state
        .commands()
        .mark_sent(command.id, tenant_id, agent_id)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        crate::repositories::RepositoryError::InvalidCommandTransition { from, action }
            if from == CommandStatus::Acknowledged.as_str() && action == "send"
    ));

    sender
        .send(Ok(success_event(tenant_id, agent_id, command.id)))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let err = state
        .commands()
        .mark_acknowledged(command.id, tenant_id, agent_id)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        crate::repositories::RepositoryError::InvalidCommandTransition { from, action }
            if from == CommandStatus::Succeeded.as_str() && action == "acknowledge"
    ));
}

#[tokio::test]
async fn grpc_hub_command_from_record_maps_discovery_and_diagnostics() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let discovery = state
        .commands()
        .enqueue_discover_printers(
            tenant_id,
            agent_id,
            DiscoverPrintersPayload { timeout_seconds: 7 },
        )
        .await
        .unwrap();
    let diagnostic = state
        .commands()
        .enqueue_diagnose_printer(
            tenant_id,
            agent_id,
            DiagnosePrinterPayload {
                serial_number: "SERIAL123".to_owned(),
            },
        )
        .await
        .unwrap();

    let discovery_command = hub_command_from_record(discovery).unwrap();
    assert!(matches!(
        discovery_command.command,
        Some(hub_command::Command::DiscoverPrinters(command)) if command.timeout_seconds == 7
    ));
    let diagnostic_command = hub_command_from_record(diagnostic).unwrap();
    assert!(matches!(
        diagnostic_command.command,
        Some(hub_command::Command::DiagnosePrinter(command)) if command.serial_number == "SERIAL123"
    ));
}

#[tokio::test]
async fn grpc_command_result_persists_result_json() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let command = state
        .commands()
        .enqueue_diagnose_printer(
            tenant_id,
            agent_id,
            DiagnosePrinterPayload {
                serial_number: "SERIAL123".to_owned(),
            },
        )
        .await
        .unwrap();
    state
        .commands()
        .mark_sent(command.id, tenant_id, agent_id)
        .await
        .unwrap();
    state
        .commands()
        .mark_acknowledged(command.id, tenant_id, agent_id)
        .await
        .unwrap();
    let result_json = r#"{"type":"printer_diagnostic","overall":"problem"}"#;

    handle_result(
        &state,
        tenant_id,
        agent_id,
        CommandResult {
            command_id: command.id.to_string(),
            success: true,
            error: String::new(),
            result_json: result_json.to_owned(),
        },
    )
    .await
    .unwrap();

    let persisted = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.status, CommandStatus::Succeeded);
    assert_eq!(persisted.result_json.as_deref(), Some(result_json));
}

#[tokio::test]
async fn grpc_unknown_command_ack_streams_not_found() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    sender
        .send(Ok(ack_event(tenant_id, agent_id, CommandId::new())))
        .await
        .unwrap();
    let err = stream.next().await.unwrap().unwrap_err();

    assert_eq!(err.code(), Code::NotFound);
}

#[tokio::test]
async fn grpc_stale_ack_is_failed_precondition() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let command = state
        .commands()
        .enqueue_refresh_printers(tenant_id, agent_id)
        .await
        .unwrap();

    let err = handle_ack(
        &state,
        tenant_id,
        agent_id,
        CommandAck {
            command_id: command.id.to_string(),
            accepted: true,
            error: String::new(),
        },
    )
    .await
    .unwrap_err();

    assert_eq!(err.code(), Code::FailedPrecondition);
}

#[tokio::test]
async fn grpc_stale_ack_streams_failed_precondition() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let command_id = sent_command(&state, tenant_id, agent_id).await;
    state
        .commands()
        .mark_failed(command_id, tenant_id, agent_id, "first")
        .await
        .unwrap();
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    sender
        .send(Ok(ack_event(tenant_id, agent_id, command_id)))
        .await
        .unwrap();
    let err = stream.next().await.unwrap().unwrap_err();

    assert_eq!(err.code(), Code::FailedPrecondition);
}
