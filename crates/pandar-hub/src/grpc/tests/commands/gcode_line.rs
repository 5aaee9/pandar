use std::{collections::HashSet, sync::Arc};

use pandar_core::{CommandRecord, CommandStatus};
use tokio::sync::{Mutex, mpsc};

use super::*;
use crate::{
    grpc::commands::{
        CommandConversionOptions, SessionQueuedDispatch, dispatch_next_queued_for_session,
        hub_command_from_record, required_feature_dispatch_pause,
    },
    repositories::{AuditActor, PrinterOperationKind},
    sessions::{AgentSession, SessionToken, empty_pending_live_commands},
};
use pandar_protocol::agent::v1::{AgentCapability, HubCommand, hub_command, printer_operation};

const EXACT_PARAM: &str = "M620 C1 \r\n; keep trailing  \n\n";

#[tokio::test]
async fn gcode_line_conversion_preserves_exact_param_without_required_features() {
    let fixture = GcodeDispatchFixture::new().await;
    let command = fixture.enqueue_gcode_line(EXACT_PARAM).await;

    let converted = hub_command_from_record(command).unwrap();

    let Some(hub_command::Command::PrinterOperation(operation)) = converted.command else {
        panic!("expected printer operation command");
    };
    assert!(operation.required_device_features.is_empty());
    assert!(matches!(
        operation.operation,
        Some(printer_operation::Operation::GcodeLine(operation))
            if operation.param == EXACT_PARAM
    ));
}

#[tokio::test]
async fn gcode_line_current_capable_session_receives_exact_param() {
    let fixture = GcodeDispatchFixture::new().await;
    let token = SessionToken::new();
    let (sender, mut receiver) = fixture
        .register_session(token, [AgentCapability::GcodeLine])
        .await;
    let command = fixture.enqueue_gcode_line(EXACT_PARAM).await;

    let outcome = fixture.dispatch(token, &sender).await;

    assert_eq!(outcome, SessionQueuedDispatch::Sent);
    let emitted = receiver.recv().await.unwrap().unwrap();
    assert_eq!(emitted.command_id, command.id.to_string());
    let Some(hub_command::Command::PrinterOperation(operation)) = emitted.command else {
        panic!("expected printer operation command");
    };
    assert!(matches!(
        operation.operation,
        Some(printer_operation::Operation::GcodeLine(operation))
            if operation.param == EXACT_PARAM
    ));
    fixture
        .assert_status(command, CommandStatus::Sent, None)
        .await;
}

#[tokio::test]
async fn gcode_line_current_incapable_session_fails_without_sending() {
    let fixture = GcodeDispatchFixture::new().await;
    let token = SessionToken::new();
    let (sender, mut receiver) = fixture.register_session(token, []).await;
    let command = fixture.enqueue_gcode_line(EXACT_PARAM).await;

    let outcome = fixture.dispatch(token, &sender).await;

    assert_eq!(outcome, SessionQueuedDispatch::FailedAndContinue);
    assert!(receiver.try_recv().is_err());
    fixture
        .assert_failed(
            command,
            "agent capability gate failed: current agent session does not advertise gcode-line capability",
        )
        .await;
}

#[tokio::test]
async fn gcode_line_stale_session_leaves_row_queued_for_capable_replacement() {
    let fixture = GcodeDispatchFixture::new().await;
    let old_token = SessionToken::new();
    let (old_sender, mut old_receiver) = fixture
        .register_session(old_token, [AgentCapability::GcodeLine])
        .await;
    let command = fixture.enqueue_gcode_line(EXACT_PARAM).await;
    let replacement = SessionToken::new();
    let (replacement_sender, mut replacement_receiver) = fixture
        .register_session(replacement, [AgentCapability::GcodeLine])
        .await;

    let stale_outcome = fixture.dispatch(old_token, &old_sender).await;

    assert_eq!(stale_outcome, SessionQueuedDispatch::SessionEnded);
    assert!(old_receiver.try_recv().is_err());
    fixture
        .assert_status(command.clone(), CommandStatus::Queued, None)
        .await;

    let replacement_outcome = fixture.dispatch(replacement, &replacement_sender).await;

    assert_eq!(replacement_outcome, SessionQueuedDispatch::Sent);
    let emitted = replacement_receiver.recv().await.unwrap().unwrap();
    assert_eq!(emitted.command_id, command.id.to_string());
    fixture
        .assert_status(command, CommandStatus::Sent, None)
        .await;
}

#[tokio::test]
async fn gcode_line_sent_row_is_not_replayed_after_disconnect_and_replacement() {
    let fixture = GcodeDispatchFixture::new().await;
    let token = SessionToken::new();
    let (sender, mut receiver) = fixture
        .register_session(token, [AgentCapability::GcodeLine])
        .await;
    let command = fixture.enqueue_gcode_line(EXACT_PARAM).await;

    assert_eq!(
        fixture.dispatch(token, &sender).await,
        SessionQueuedDispatch::Sent
    );
    assert_eq!(
        receiver.recv().await.unwrap().unwrap().command_id,
        command.id.to_string()
    );
    crate::grpc::disconnect_session(&fixture.state, fixture.tenant_id, fixture.agent_id, token)
        .await
        .unwrap();
    let replacement = SessionToken::new();
    let (replacement_sender, mut replacement_receiver) = fixture
        .register_session(replacement, [AgentCapability::GcodeLine])
        .await;

    let replacement_outcome = fixture.dispatch(replacement, &replacement_sender).await;

    assert_eq!(replacement_outcome, SessionQueuedDispatch::Empty);
    assert!(replacement_receiver.try_recv().is_err());
    fixture
        .assert_status(command, CommandStatus::Sent, None)
        .await;
}

#[tokio::test]
async fn gcode_line_replacement_waits_for_dispatch_transition_lease() {
    let fixture = GcodeDispatchFixture::new().await;
    let token = SessionToken::new();
    let (sender, mut receiver) = fixture
        .register_session(token, [AgentCapability::GcodeLine])
        .await;
    let command = fixture.enqueue_gcode_line(EXACT_PARAM).await;
    let mut pause = required_feature_dispatch_pause::install(
        token,
        required_feature_dispatch_pause::Phase::AfterFeatureValidation,
    );
    let dispatch_fixture = fixture.clone();
    let dispatch_sender = sender.clone();
    let dispatch =
        tokio::spawn(async move { dispatch_fixture.dispatch(token, &dispatch_sender).await });
    pause.wait_until_reached().await;

    let replacement = SessionToken::new();
    let mut waiting = crate::sessions::transition_pause::observe_waiting(replacement);
    let replacement_fixture = fixture.clone();
    let replacement_task = tokio::spawn(async move {
        replacement_fixture
            .register_session(replacement, [AgentCapability::GcodeLine])
            .await
    });
    waiting.wait_until_reached().await;
    assert!(!replacement_task.is_finished());
    assert!(receiver.try_recv().is_err());

    pause.resume();
    assert_eq!(dispatch.await.unwrap(), SessionQueuedDispatch::Sent);
    assert_eq!(
        receiver.recv().await.unwrap().unwrap().command_id,
        command.id.to_string()
    );
    replacement_task.await.unwrap();
}

#[tokio::test]
async fn gcode_line_gate_leaves_non_gcode_dispatch_unchanged() {
    let fixture = GcodeDispatchFixture::new().await;
    let token = SessionToken::new();
    let (sender, mut receiver) = fixture.register_session(token, []).await;
    let command = fixture.enqueue_pause().await;

    let outcome = fixture.dispatch(token, &sender).await;

    assert_eq!(outcome, SessionQueuedDispatch::Sent);
    assert_eq!(
        receiver.recv().await.unwrap().unwrap().command_id,
        command.id.to_string()
    );
    fixture
        .assert_status(command, CommandStatus::Sent, None)
        .await;
}

#[tokio::test]
async fn gcode_line_gate_preserves_required_device_feature_error_prefix() {
    let fixture = GcodeDispatchFixture::new().await;
    let token = SessionToken::new();
    let (sender, mut receiver) = fixture.register_session(token, []).await;
    let command = fixture.enqueue_required_home().await;

    let outcome = fixture.dispatch(token, &sender).await;

    assert_eq!(outcome, SessionQueuedDispatch::FailedAndContinue);
    assert!(receiver.try_recv().is_err());
    fixture
        .assert_failed(
            command,
            "required device feature gate failed: current agent session does not advertise required-device-features capability",
        )
        .await;
}

#[derive(Clone)]
struct GcodeDispatchFixture {
    state: AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    printer_id: String,
}

impl GcodeDispatchFixture {
    async fn new() -> Self {
        let state = fixture_state().await;
        let (tenant_id, agent_id) = tenant_agent(&state).await;
        let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
            state.database(),
            tenant_id,
            agent_id,
            Some("A1"),
        )
        .await
        .unwrap();
        Self {
            state,
            tenant_id,
            agent_id,
            printer_id,
        }
    }

    async fn register_session(
        &self,
        token: SessionToken,
        capabilities: impl IntoIterator<Item = AgentCapability>,
    ) -> (
        mpsc::Sender<Result<HubCommand, tonic::Status>>,
        mpsc::Receiver<Result<HubCommand, tonic::Status>>,
    ) {
        let (wake_sender, _) = mpsc::channel(1);
        let (close_sender, _) = mpsc::channel(1);
        let (command_sender, command_receiver) = mpsc::channel(4);
        let now = pandar_core::created_at_now();
        let _lease = self
            .state
            .sessions()
            .transition_lease_for_session(self.agent_id, token)
            .await;
        self.state
            .agents()
            .claim_online_session(
                self.tenant_id,
                self.agent_id,
                &token.persisted_id(),
                "test",
                &now,
            )
            .await
            .unwrap();
        self.state
            .sessions()
            .register(AgentSession {
                token,
                tenant_id: self.tenant_id,
                agent_id: self.agent_id,
                name: "agent".to_owned(),
                version: "test".to_owned(),
                connected_at: now.clone(),
                last_heartbeat_at: now,
                wake_sender,
                close_sender,
                command_sender: command_sender.clone(),
                capabilities: capabilities.into_iter().collect::<HashSet<_>>(),
                pending_live_commands: empty_pending_live_commands(),
                live_command_transition: Arc::new(Mutex::new(())),
            })
            .await;
        (command_sender, command_receiver)
    }

    async fn enqueue_gcode_line(&self, param: &str) -> CommandRecord {
        self.enqueue(PrinterOperationKind::GcodeLine {
            param: param.to_owned(),
        })
        .await
    }

    async fn enqueue_pause(&self) -> CommandRecord {
        self.enqueue(PrinterOperationKind::Pause {}).await
    }

    async fn enqueue_required_home(&self) -> CommandRecord {
        let operation = serde_json::from_value(serde_json::json!({
            "type": "home",
            "axes": [],
            "required_device_features": ["bambu_mqtt_homing"]
        }))
        .unwrap();
        self.enqueue(operation).await
    }

    async fn enqueue(&self, operation: PrinterOperationKind) -> CommandRecord {
        self.state
            .commands()
            .enqueue_printer_operation_with_audit(
                self.tenant_id,
                &self.printer_id,
                operation,
                test_actor(),
            )
            .await
            .unwrap()
    }

    async fn dispatch(
        &self,
        token: SessionToken,
        sender: &mpsc::Sender<Result<HubCommand, tonic::Status>>,
    ) -> SessionQueuedDispatch {
        dispatch_next_queued_for_session(
            &self.state,
            self.tenant_id,
            self.agent_id,
            token,
            sender,
            conversion_options(),
        )
        .await
        .unwrap()
    }

    async fn assert_failed(&self, command: CommandRecord, expected_error: &str) {
        self.assert_status(command, CommandStatus::Failed, Some(expected_error))
            .await;
    }

    async fn assert_status(
        &self,
        command: CommandRecord,
        status: CommandStatus,
        error: Option<&str>,
    ) {
        let persisted = self
            .state
            .commands()
            .get_for_tenant(self.tenant_id, command.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(persisted.status, status);
        assert_eq!(persisted.error.as_deref(), error);
    }
}

fn conversion_options() -> CommandConversionOptions {
    CommandConversionOptions {
        require_artifact_download_path: false,
    }
}

fn test_actor() -> AuditActor {
    AuditActor::tenant_token(None, "gcode-line-test", vec!["*"])
}
