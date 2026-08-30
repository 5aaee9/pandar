use std::{collections::HashSet, sync::Arc};

use pandar_core::{BambuDeviceFeatures, CommandRecord, CommandStatus};
use tokio::sync::{Mutex, mpsc};

use super::*;
use crate::{
    grpc::commands::{
        SessionQueuedDispatch, dispatch_next_queued_for_session, required_feature_dispatch_pause,
    },
    repositories::{AuditActor, PrinterOperationKind},
    sessions::{AgentSession, SessionToken, empty_pending_live_commands},
};
use pandar_protocol::agent::v1::{AgentCapability, DeviceFeature, hub_command};

const HOMING_BITS: u64 = 1_u64 << 32;

#[tokio::test]
async fn required_device_features_dispatch_only_to_matching_capable_current_session() {
    let fixture = DispatchFixture::new().await;
    let token = SessionToken::new();
    let (sender, mut receiver) = fixture
        .register_session(token, [AgentCapability::RequiredDeviceFeatures])
        .await;
    fixture
        .observe_features(token, BambuDeviceFeatures::from_bits(HOMING_BITS))
        .await;
    let command = fixture.enqueue_required_home().await;

    let outcome = dispatch_next_queued_for_session(
        &fixture.state,
        fixture.tenant_id,
        fixture.agent_id,
        token,
        &sender,
    )
    .await
    .unwrap();

    assert_eq!(outcome, SessionQueuedDispatch::Sent);
    let emitted = receiver.recv().await.unwrap().unwrap();
    assert_eq!(emitted.command_id, command.id.to_string());
    let Some(hub_command::Command::PrinterOperation(operation)) = emitted.command else {
        panic!("expected printer operation command");
    };
    assert_eq!(
        operation.required_device_features,
        [DeviceFeature::BambuMqttHoming as i32]
    );
    fixture
        .assert_status(command, CommandStatus::Sent, None)
        .await;
}

#[tokio::test]
async fn required_device_features_fail_for_incapable_replacement_without_sending() {
    let fixture = DispatchFixture::new().await;
    let old_token = SessionToken::new();
    fixture
        .register_session(old_token, [AgentCapability::RequiredDeviceFeatures])
        .await;
    fixture
        .observe_features(old_token, BambuDeviceFeatures::from_bits(HOMING_BITS))
        .await;
    let command = fixture.enqueue_required_home().await;
    let replacement = SessionToken::new();
    let (sender, mut receiver) = fixture.register_session(replacement, []).await;

    let outcome = fixture.dispatch(replacement, &sender).await;

    assert_eq!(outcome, SessionQueuedDispatch::FailedAndContinue);
    assert!(receiver.try_recv().is_err());
    fixture
        .assert_failed(
            command,
            "does not advertise required-device-features capability",
        )
        .await;
}

#[tokio::test]
async fn required_device_features_fail_for_capable_replacement_with_old_observation_marker() {
    let fixture = DispatchFixture::new().await;
    let old_token = SessionToken::new();
    fixture
        .register_session(old_token, [AgentCapability::RequiredDeviceFeatures])
        .await;
    fixture
        .observe_features(old_token, BambuDeviceFeatures::from_bits(HOMING_BITS))
        .await;
    let command = fixture.enqueue_required_home().await;
    let replacement = SessionToken::new();
    let (sender, mut receiver) = fixture
        .register_session(replacement, [AgentCapability::RequiredDeviceFeatures])
        .await;

    let outcome = fixture.dispatch(replacement, &sender).await;

    assert_eq!(outcome, SessionQueuedDispatch::FailedAndContinue);
    assert!(receiver.try_recv().is_err());
    fixture
        .assert_failed(
            command,
            "feature observation belongs to a different agent session",
        )
        .await;
}

#[tokio::test]
async fn required_device_features_fail_when_matching_observation_lacks_bit() {
    let fixture = DispatchFixture::new().await;
    let token = SessionToken::new();
    let (sender, mut receiver) = fixture
        .register_session(token, [AgentCapability::RequiredDeviceFeatures])
        .await;
    fixture
        .observe_features(token, BambuDeviceFeatures::default())
        .await;
    let command = fixture.enqueue_required_home().await;

    let outcome = fixture.dispatch(token, &sender).await;

    assert_eq!(outcome, SessionQueuedDispatch::FailedAndContinue);
    assert!(receiver.try_recv().is_err());
    fixture
        .assert_failed(command, "missing bambu_mqtt_homing")
        .await;
}

#[tokio::test]
async fn required_device_features_fail_when_observation_marker_is_missing() {
    let fixture = DispatchFixture::new().await;
    let token = SessionToken::new();
    let (sender, mut receiver) = fixture
        .register_session(token, [AgentCapability::RequiredDeviceFeatures])
        .await;
    let command = fixture.enqueue_required_home().await;

    let outcome = fixture.dispatch(token, &sender).await;

    assert_eq!(outcome, SessionQueuedDispatch::FailedAndContinue);
    assert!(receiver.try_recv().is_err());
    fixture
        .assert_failed(command, "feature observation has no agent-session marker")
        .await;
}

#[tokio::test]
async fn required_device_features_fail_malformed_persisted_other_action_without_sending() {
    let fixture = DispatchFixture::new().await;
    let token = SessionToken::new();
    let (sender, mut receiver) = fixture.register_session(token, []).await;
    let command = fixture.enqueue_required_home().await;
    let mut payload: serde_json::Value = serde_json::from_str(&command.payload_json).unwrap();
    payload["operation"] = serde_json::json!({
        "type": "pause",
        "required_device_features": ["bambu_mqtt_homing"]
    });
    let crate::db::Database::Sqlite(pool) = fixture.state.database() else {
        panic!("expected SQLite database");
    };
    sqlx::query("UPDATE commands SET payload_json = ?2 WHERE id = ?1")
        .bind(command.id.to_string())
        .bind(serde_json::to_string(&payload).unwrap())
        .execute(pool)
        .await
        .unwrap();

    let outcome = fixture.dispatch(token, &sender).await;

    assert_eq!(outcome, SessionQueuedDispatch::FailedAndContinue);
    assert!(receiver.try_recv().is_err());
    fixture
        .assert_failed(
            command,
            "persisted printer operation payload is invalid: unknown field `required_device_features`",
        )
        .await;
}

#[tokio::test]
async fn required_device_features_fail_after_disconnect_without_sending() {
    let fixture = DispatchFixture::new().await;
    let token = SessionToken::new();
    let (sender, mut receiver) = fixture
        .register_session(token, [AgentCapability::RequiredDeviceFeatures])
        .await;
    fixture
        .observe_features(token, BambuDeviceFeatures::from_bits(HOMING_BITS))
        .await;
    let command = fixture.enqueue_required_home().await;
    crate::grpc::disconnect_session(&fixture.state, fixture.tenant_id, fixture.agent_id, token)
        .await
        .unwrap();

    let outcome = fixture.dispatch(token, &sender).await;

    assert_eq!(outcome, SessionQueuedDispatch::FailedAndContinue);
    assert!(receiver.try_recv().is_err());
    fixture
        .assert_failed(command, "exact agent session is no longer current")
        .await;
}

#[tokio::test]
async fn required_device_features_idle_pump_fails_all_queued_commands_on_normal_disconnect() {
    let fixture = DispatchFixture::new().await;
    let (mut stream, inbound_sender) = connect_live(
        &fixture.state,
        vec![required_features_hello(fixture.tenant_id, fixture.agent_id)],
    )
    .await
    .unwrap();
    let token = fixture
        .state
        .sessions()
        .get(fixture.agent_id)
        .await
        .unwrap()
        .token;
    fixture
        .observe_features(token, BambuDeviceFeatures::from_bits(HOMING_BITS))
        .await;
    let legacy_before = fixture.enqueue_legacy_pause().await;
    let required_first = fixture.enqueue_required_home().await;
    let legacy_between = fixture.enqueue_legacy_pause().await;
    let required_last = fixture.enqueue_required_home().await;

    drop(inbound_sender);

    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
            .await
            .expect("timed out waiting for disconnected pump")
            .is_none()
    );
    fixture
        .assert_status(legacy_before, CommandStatus::Queued, None)
        .await;
    fixture
        .assert_failed(required_first, "exact agent session is no longer current")
        .await;
    fixture
        .assert_status(legacy_between, CommandStatus::Queued, None)
        .await;
    fixture
        .assert_failed(required_last, "exact agent session is no longer current")
        .await;
}

#[tokio::test]
async fn required_device_features_old_closer_preserves_replacement_queue() {
    let fixture = DispatchFixture::new().await;
    let old_token = SessionToken::new();
    fixture
        .register_session(old_token, [AgentCapability::RequiredDeviceFeatures])
        .await;
    let legacy = fixture.enqueue_legacy_pause().await;
    let required = fixture.enqueue_required_home().await;
    let replacement = SessionToken::new();
    fixture
        .register_session(replacement, [AgentCapability::RequiredDeviceFeatures])
        .await;

    crate::grpc::commands::finalize_required_features_for_closing_session(
        &fixture.state,
        fixture.tenant_id,
        fixture.agent_id,
        old_token,
    )
    .await
    .unwrap();

    fixture
        .assert_status(legacy, CommandStatus::Queued, None)
        .await;
    fixture
        .assert_status(required, CommandStatus::Queued, None)
        .await;
}

#[tokio::test]
async fn required_device_features_outbound_ready_does_not_wait_for_entire_queue() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    for _ in 0..17 {
        state
            .commands()
            .enqueue_refresh_printers(tenant_id, agent_id)
            .await
            .unwrap();
    }

    let (stream, inbound_sender) = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        connect_live(&state, vec![hello_event(tenant_id, agent_id)]),
    )
    .await
    .expect("connect must not wait for the bounded outbound queue to drain")
    .unwrap();

    drop(stream);
    drop(inbound_sender);
}

#[tokio::test]
async fn required_device_features_leave_requirement_free_dispatch_unchanged() {
    let fixture = DispatchFixture::new().await;
    let token = SessionToken::new();
    let (sender, mut receiver) = fixture.register_session(token, []).await;
    let command = fixture
        .state
        .commands()
        .enqueue_printer_operation_with_audit(
            fixture.tenant_id,
            &fixture.printer_id,
            PrinterOperationKind::Pause {},
            test_actor(),
        )
        .await
        .unwrap();

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
async fn required_device_features_hold_session_lease_at_every_dispatch_pause() {
    for phase in [
        required_feature_dispatch_pause::Phase::AfterQueuedRowRead,
        required_feature_dispatch_pause::Phase::AfterFeatureValidation,
        required_feature_dispatch_pause::Phase::BeforeChannelSend,
    ] {
        assert_replacement_waits_at(phase).await;
    }
}

async fn assert_replacement_waits_at(phase: required_feature_dispatch_pause::Phase) {
    let fixture = DispatchFixture::new().await;
    let token = SessionToken::new();
    let (sender, mut receiver) = fixture
        .register_session(token, [AgentCapability::RequiredDeviceFeatures])
        .await;
    fixture
        .observe_features(token, BambuDeviceFeatures::from_bits(HOMING_BITS))
        .await;
    let command = fixture.enqueue_required_home().await;
    let mut pause = required_feature_dispatch_pause::install(token, phase);
    let dispatch_state = fixture.state.clone();
    let dispatch_sender = sender.clone();
    let dispatch = tokio::spawn(async move {
        dispatch_next_queued_for_session(
            &dispatch_state,
            command.tenant_id,
            command.agent_id,
            token,
            &dispatch_sender,
        )
        .await
        .unwrap()
    });
    pause.wait_until_reached().await;

    let replacement = SessionToken::new();
    let mut waiting = crate::sessions::transition_pause::observe_waiting(replacement);
    let replacement_fixture = fixture.clone();
    let replacement_task = tokio::spawn(async move {
        replacement_fixture
            .register_session(replacement, [AgentCapability::RequiredDeviceFeatures])
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

#[derive(Clone)]
struct DispatchFixture {
    state: AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    printer_id: String,
}

impl DispatchFixture {
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

    async fn observe_features(&self, token: SessionToken, features: BambuDeviceFeatures) {
        let outcome = self
            .state
            .printers()
            .update_device_features_if_current(
                self.tenant_id,
                self.agent_id,
                &token.persisted_id(),
                &format!("serial-{}", self.printer_id),
                Some(features),
            )
            .await
            .unwrap();
        assert_eq!(
            outcome,
            crate::repositories::DeviceFeatureUpdateOutcome::Updated
        );
    }

    async fn enqueue_required_home(&self) -> CommandRecord {
        let operation: PrinterOperationKind = serde_json::from_value(serde_json::json!({
            "type": "home",
            "axes": [],
            "required_device_features": ["bambu_mqtt_homing"]
        }))
        .unwrap();
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

    async fn enqueue_legacy_pause(&self) -> CommandRecord {
        self.state
            .commands()
            .enqueue_printer_operation_with_audit(
                self.tenant_id,
                &self.printer_id,
                PrinterOperationKind::Pause {},
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
        dispatch_next_queued_for_session(&self.state, self.tenant_id, self.agent_id, token, sender)
            .await
            .unwrap()
    }

    async fn assert_failed(&self, command: CommandRecord, expected_context: &str) {
        let persisted = self
            .state
            .commands()
            .get_for_tenant(self.tenant_id, command.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(persisted.status, CommandStatus::Failed);
        let error = persisted.error.unwrap();
        assert!(error.starts_with("required device feature gate failed:"));
        assert!(error.contains(expected_context), "{error}");
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

fn required_features_hello(tenant_id: TenantId, agent_id: AgentId) -> AgentEvent {
    let mut event = hello_event_with_credential(tenant_id, agent_id, TEST_AGENT_CREDENTIAL);
    let Some(agent_event::Event::Hello(hello)) = &mut event.event else {
        unreachable!("hello helper must build AgentHello");
    };
    hello.capabilities = vec![AgentCapability::RequiredDeviceFeatures as i32];
    event
}

fn test_actor() -> AuditActor {
    AuditActor::tenant_token(None, "required-features-test", vec!["*"])
}
