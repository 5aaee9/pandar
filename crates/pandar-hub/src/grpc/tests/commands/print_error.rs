use std::time::Duration;

use pandar_core::{AgentId, CommandId, CommandRecord, CommandRecordParts, CommandStatus, TenantId};
use prost::Message;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tonic::Code;
use tonic::Status;

use crate::{
    AppState,
    grpc::commands::{hub_command_from_record, live_printer_operation_hub_command},
    protocol::agent::v1::{
        AgentCapability, AgentEvent, CommandAck, CommandResult,
        PrintErrorAction as ProtoPrintErrorAction, agent_event, hub_command, printer_operation,
    },
    repositories::{PrintErrorAction, PrinterOperationKind, PrinterOperationPayload},
};

mod reconnect;

#[test]
fn live_print_error_builder_maps_all_actions_and_wire_tag_25() {
    for (action, expected) in [
        (PrintErrorAction::Resume, ProtoPrintErrorAction::Resume),
        (PrintErrorAction::Ignore, ProtoPrintErrorAction::Ignore),
        (PrintErrorAction::Stop, ProtoPrintErrorAction::Stop),
    ] {
        let command_id = CommandId::new();
        let command = live_printer_operation_hub_command(
            command_id,
            "SERIAL123".to_owned(),
            native_operation(action),
        );

        assert_eq!(command.command_id, command_id.to_string());
        let Some(hub_command::Command::PrinterOperation(operation)) = command.command else {
            panic!("expected printer operation command");
        };
        assert_eq!(operation.serial_number, "SERIAL123");
        assert!(
            operation
                .encode_to_vec()
                .windows(2)
                .any(|window| window == [0xca, 0x01]),
            "handle_print_error must use oneof tag 25"
        );
        let Some(printer_operation::Operation::HandlePrintError(operation)) = operation.operation
        else {
            panic!("expected handle print error operation");
        };
        assert_eq!(operation.error_action, expected as i32);
        assert_eq!(operation.print_error, 83_918_929);
        assert_eq!(operation.printer_job_id, "job-7");
        assert_eq!(operation.sequence_id, 20_042);
    }
}

#[test]
fn durable_conversion_rejects_live_print_error_operation() {
    let payload = PrinterOperationPayload {
        printer_id: "printer-1".to_owned(),
        serial_number: "SERIAL123".to_owned(),
        operation: native_operation(PrintErrorAction::Resume),
    };
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        printer_id: Some(payload.printer_id.clone()),
        kind: "printer_operation".to_owned(),
        status: CommandStatus::Queued.to_string(),
        payload_json: serde_json::to_string(&payload).unwrap(),
        result_json: None,
        error: None,
        created_at: "2026-01-01T00:00:00Z".to_owned(),
        updated_at: "2026-01-01T00:00:00Z".to_owned(),
    })
    .unwrap();

    let error = hub_command_from_record(command).unwrap_err();

    assert_eq!(error.code(), Code::FailedPrecondition);
    assert_eq!(
        error.message(),
        "print error operation requires live dispatch"
    );
}

#[tokio::test]
async fn live_print_error_ack_waits_for_the_session_transition_and_stays_pending() {
    let fixture = live_fixture().await;
    let transition = fixture
        .state
        .sessions()
        .get(fixture.agent_id)
        .await
        .unwrap()
        .live_command_transition;
    let transition = transition.lock_owned().await;

    fixture
        .sender
        .send(Ok(command_ack_event(
            fixture.tenant_id,
            fixture.agent_id,
            fixture.command.id,
            true,
        )))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(25)).await;

    assert_eq!(fixture.command().await.status, CommandStatus::Sent);
    assert!(fixture.pending());

    drop(transition);
    assert_eq!(
        fixture
            .wait_for_status(CommandStatus::Acknowledged)
            .await
            .status,
        CommandStatus::Acknowledged
    );
    assert!(fixture.pending());
}

#[tokio::test]
async fn live_print_error_result_waits_for_the_session_transition_and_removes_pending() {
    let fixture = live_fixture().await;
    let transition = fixture
        .state
        .sessions()
        .get(fixture.agent_id)
        .await
        .unwrap()
        .live_command_transition;
    let transition = transition.lock_owned().await;

    fixture
        .sender
        .send(Ok(command_result_event(
            fixture.tenant_id,
            fixture.agent_id,
            fixture.command.id,
        )))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(25)).await;

    assert_eq!(fixture.command().await.status, CommandStatus::Sent);
    assert!(fixture.pending());

    drop(transition);
    assert_eq!(
        fixture
            .wait_for_status(CommandStatus::Succeeded)
            .await
            .status,
        CommandStatus::Succeeded
    );
    fixture.wait_for_pending(false).await;
}

#[tokio::test]
async fn sequence_zero_puback_result_terminalizes_only_the_live_command() {
    let fixture = live_fixture_for(
        PrinterOperationKind::HandlePrintError {
            error_action: PrintErrorAction::Resume,
            print_error: 83_918_929,
            printer_job_id: "job-7".to_owned(),
            sequence_id: 0,
        },
        AgentCapability::HandlePrintErrorSequenceZeroPubackOnly,
    )
    .await;

    fixture
        .sender
        .send(Ok(command_result_event(
            fixture.tenant_id,
            fixture.agent_id,
            fixture.command.id,
        )))
        .await
        .unwrap();

    assert_eq!(
        fixture
            .wait_for_status(CommandStatus::Succeeded)
            .await
            .status,
        CommandStatus::Succeeded
    );
    fixture.wait_for_pending(false).await;
}

fn native_operation(error_action: PrintErrorAction) -> PrinterOperationKind {
    PrinterOperationKind::HandlePrintError {
        error_action,
        print_error: 83_918_929,
        printer_job_id: "job-7".to_owned(),
        sequence_id: 20_042,
    }
}

struct LiveFixture {
    state: AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    command: CommandRecord,
    pending: crate::sessions::PendingLiveCommands,
    _stream: crate::grpc::ResponseStream,
    sender: mpsc::Sender<Result<AgentEvent, Status>>,
}

impl LiveFixture {
    async fn command(&self) -> CommandRecord {
        self.state
            .commands()
            .get_for_tenant(self.tenant_id, self.command.id)
            .await
            .unwrap()
            .unwrap()
    }

    fn pending(&self) -> bool {
        self.pending.lock().unwrap().contains_key(&self.command.id)
    }

    async fn wait_for_pending(&self, expected: bool) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while self.pending() != expected {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    async fn wait_for_status(&self, status: CommandStatus) -> CommandRecord {
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let command = self.command().await;
                if command.status == status {
                    return command;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap()
    }
}

async fn live_fixture() -> LiveFixture {
    live_fixture_for(
        native_operation(PrintErrorAction::Resume),
        AgentCapability::HandlePrintError,
    )
    .await
}

async fn live_fixture_for(
    operation: PrinterOperationKind,
    capability: AgentCapability,
) -> LiveFixture {
    let state = super::fixture_state().await;
    let (tenant_id, agent_id) = super::tenant_agent(&state).await;
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("A1"),
    )
    .await
    .unwrap();
    let command = state
        .commands()
        .create_printer_operation_sent_with_audit(
            tenant_id,
            &printer_id,
            agent_id,
            operation.clone(),
            super::test_audit_actor(),
        )
        .await
        .unwrap();
    let (mut stream, sender) = super::connect_live(
        &state,
        vec![hello_event_with_capability(tenant_id, agent_id, capability)],
    )
    .await
    .unwrap();
    let session = state.sessions().get(agent_id).await.unwrap();
    let token = session.token;
    let pending = session.pending_live_commands;
    state
        .sessions()
        .try_dispatch_live_command_with_capability(
            tenant_id,
            agent_id,
            token,
            capability,
            command.id,
            live_printer_operation_hub_command(
                command.id,
                format!("serial-{printer_id}"),
                operation,
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        stream.next().await.unwrap().unwrap().command_id,
        command.id.to_string()
    );

    LiveFixture {
        state,
        tenant_id,
        agent_id,
        command,
        pending,
        _stream: stream,
        sender,
    }
}

fn capable_hello_event(tenant_id: TenantId, agent_id: AgentId) -> AgentEvent {
    hello_event_with_capability(tenant_id, agent_id, AgentCapability::HandlePrintError)
}

fn hello_event_with_capability(
    tenant_id: TenantId,
    agent_id: AgentId,
    capability: AgentCapability,
) -> AgentEvent {
    let mut event = super::hello_event(tenant_id, agent_id);
    let Some(agent_event::Event::Hello(hello)) = event.event.as_mut() else {
        panic!("hello event");
    };
    hello.capabilities = vec![capability as i32];
    event
}

fn command_ack_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
    accepted: bool,
) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_owned(),
        event: Some(agent_event::Event::CommandAck(CommandAck {
            command_id: command_id.to_string(),
            accepted,
            error: String::new(),
        })),
    }
}

fn command_result_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_owned(),
        event: Some(agent_event::Event::CommandResult(CommandResult {
            command_id: command_id.to_string(),
            success: true,
            error: String::new(),
            result_json: String::new(),
        })),
    }
}
