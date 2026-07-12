use std::{collections::HashSet, sync::Arc, time::Duration};

use pandar_core::{AgentId, CommandId, CommandStatus, TenantId};
use sea_orm::EntityTrait;
use serde::Deserialize;
use tokio::sync::{Mutex, mpsc};

use super::*;
use crate::{
    protocol::agent::v1::{
        AgentCapability, PrintErrorAction as ProtoPrintErrorAction, hub_command, printer_operation,
    },
    repositories::{PrintErrorAction, PrinterOperationKind, PrinterOperationPayload},
    sessions::{AgentSession, SessionToken, empty_pending_live_commands},
};

mod ownership;
mod validation;

#[derive(Debug, Deserialize)]
struct OperationResponse {
    command_id: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct OperationErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct PrintErrorAuditMetadata {
    agent_id: String,
    serial_number: String,
    action: String,
    error_action: PrintErrorAction,
    print_error: u32,
    printer_job_id: String,
    sequence_id: u64,
    tenant_token_id: String,
    tenant_token_scopes: Vec<String>,
}

#[tokio::test]
async fn plugin_print_error_dispatches_all_actions_as_sent_tag_25_commands_without_wake() {
    let fixture = operation_fixture("plugin-native-actions").await;
    let _control_plane = start_control_plane(fixture.state.clone()).await;
    let (wake_sender, mut wake_receiver) = mpsc::channel(1);
    let (command_sender, mut command_receiver) = mpsc::channel(4);
    register_session(
        &fixture,
        wake_sender,
        command_sender,
        [AgentCapability::HandlePrintError],
    )
    .await;

    for (action, expected) in [
        ("resume", ProtoPrintErrorAction::Resume),
        ("ignore", ProtoPrintErrorAction::Ignore),
        ("stop", ProtoPrintErrorAction::Stop),
    ] {
        let (status, body) = request_as(
            fixture.app.clone(),
            Method::POST,
            &fixture.uri,
            Some(native_body(action)),
            &fixture.token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let response = decode::<OperationResponse>(body);
        assert_eq!(response.status, "sent");
        let command_id = CommandId::parse(&response.command_id).unwrap();
        let persisted = fixture
            .state
            .commands()
            .get_for_tenant(fixture.tenant_id, command_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(persisted.status, CommandStatus::Sent);
        let payload: PrinterOperationPayload =
            serde_json::from_str(&persisted.payload_json).unwrap();
        assert_eq!(
            payload.operation,
            PrinterOperationKind::HandlePrintError {
                error_action: match action {
                    "resume" => PrintErrorAction::Resume,
                    "ignore" => PrintErrorAction::Ignore,
                    "stop" => PrintErrorAction::Stop,
                    _ => unreachable!(),
                },
                print_error: 83_918_929,
                printer_job_id: "job-7".to_owned(),
                sequence_id: 20_042,
            }
        );

        let emitted = command_receiver.recv().await.unwrap().unwrap();
        let Some(hub_command::Command::PrinterOperation(operation)) = emitted.command else {
            panic!("expected printer operation command");
        };
        let Some(printer_operation::Operation::HandlePrintError(operation)) = operation.operation
        else {
            panic!("expected handle print error operation");
        };
        assert_eq!(operation.error_action, expected as i32);
        assert_eq!(operation.print_error, 83_918_929);
        assert_eq!(operation.printer_job_id, "job-7");
        assert_eq!(operation.sequence_id, 20_042);
        fixture
            .state
            .commands()
            .mark_succeeded(command_id, fixture.tenant_id, fixture.agent_id)
            .await
            .unwrap();
    }

    assert!(
        tokio::time::timeout(Duration::from_millis(100), wake_receiver.recv())
            .await
            .is_err(),
        "live printer error operations must not wake the durable pump"
    );
    let events = fixture
        .state
        .audit_events()
        .list_for_tenant(fixture.tenant_id)
        .await
        .unwrap();
    let native_events = events
        .iter()
        .filter(|event| event.action == "printer.dispatch_control")
        .collect::<Vec<_>>();
    assert_eq!(native_events.len(), 3);
    for (event, expected_action) in native_events.iter().zip(["resume", "ignore", "stop"]) {
        let metadata: PrintErrorAuditMetadata = serde_json::from_str(&event.metadata_json).unwrap();
        assert_eq!(metadata.agent_id, fixture.agent_id.to_string());
        assert_eq!(
            metadata.serial_number,
            format!("serial-{}", fixture.printer_id)
        );
        assert_eq!(metadata.action, "handle_print_error");
        assert_eq!(
            metadata.error_action,
            match expected_action {
                "resume" => PrintErrorAction::Resume,
                "ignore" => PrintErrorAction::Ignore,
                "stop" => PrintErrorAction::Stop,
                _ => unreachable!(),
            }
        );
        assert_eq!(metadata.print_error, 83_918_929);
        assert_eq!(metadata.printer_job_id, "job-7");
        assert_eq!(metadata.sequence_id, 20_042);
        assert_eq!(metadata.tenant_token_scopes, ["plugin:studio"]);
    }
}

#[tokio::test]
async fn plugin_printer_operation_rejects_string_param_on_handle_print_error_without_side_effects()
{
    assert_plugin_print_error_param_rejected(
        "plugin-native-string-param",
        serde_json::json!("M620 C1"),
    )
    .await;
}

#[tokio::test]
async fn plugin_printer_operation_rejects_null_param_on_handle_print_error_without_side_effects() {
    assert_plugin_print_error_param_rejected("plugin-native-null-param", serde_json::Value::Null)
        .await;
}

#[tokio::test]
async fn plugin_print_error_rejects_offline_and_incapable_agents_before_insert() {
    let fixture = operation_fixture("plugin-native-unavailable").await;

    assert_unavailable(&fixture).await;
    let (wake_sender, _) = mpsc::channel(1);
    let (command_sender, _command_receiver) = mpsc::channel(1);
    register_session(&fixture, wake_sender, command_sender, []).await;
    assert_unavailable(&fixture).await;
    let (wake_sender, _) = mpsc::channel(1);
    let (command_sender, _command_receiver) = mpsc::channel(1);
    register_session(
        &fixture,
        wake_sender,
        command_sender,
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    assert_unavailable(&fixture).await;

    assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn plugin_print_error_marks_committed_command_failed_when_live_dispatch_fails() {
    let fixture = operation_fixture("plugin-native-dispatch-failure").await;
    let (wake_sender, _) = mpsc::channel(1);
    let (command_sender, command_receiver) = mpsc::channel(1);
    drop(command_receiver);
    register_session(
        &fixture,
        wake_sender,
        command_sender,
        [AgentCapability::HandlePrintError],
    )
    .await;

    let (status, body) = request_as(
        fixture.app.clone(),
        Method::POST,
        &fixture.uri,
        Some(native_body("resume")),
        &fixture.token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        decode::<OperationErrorResponse>(body).error,
        "printer_operation_unavailable"
    );
    let model = crate::entities::commands::Entity::find()
        .one(&fixture.state.database().sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(model.status, "failed");
    assert!(model.error.as_deref().unwrap().contains("ChannelClosed"));
    assert!(
        fixture
            .state
            .audit_events()
            .list_for_tenant(fixture.tenant_id)
            .await
            .unwrap()
            .iter()
            .any(|event| event.action == "printer.dispatch_control")
    );
}

#[tokio::test]
async fn plugin_ordinary_operation_remains_queued_and_wakes_agent() {
    let fixture = operation_fixture("plugin-ordinary-operation").await;
    let _control_plane = start_control_plane(fixture.state.clone()).await;
    let (wake_sender, mut wake_receiver) = mpsc::channel(1);
    let (command_sender, mut command_receiver) = mpsc::channel(1);
    register_session(&fixture, wake_sender, command_sender, []).await;

    let (status, body) = request_as(
        fixture.app.clone(),
        Method::POST,
        &fixture.uri,
        Some(serde_json::json!({ "action": "pause" })),
        &fixture.token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode::<OperationResponse>(body).status, "queued");
    tokio::time::timeout(Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("ordinary operation should wake agent")
        .expect("wake channel should stay open");
    assert!(command_receiver.try_recv().is_err());
}

#[tokio::test]
async fn plugin_gcode_line_queues_and_persists_exact_and_empty_params() {
    let fixture = operation_fixture("plugin-gcode-line-exact").await;

    for (expected_count, param) in [(1, "M620 C1 \r\n; keep  \n"), (2, "")] {
        let (status, body) = request_as(
            fixture.app.clone(),
            Method::POST,
            &fixture.uri,
            Some(serde_json::json!({
                "action": "gcode_line",
                "param": param,
            })),
            &fixture.token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let response = decode::<OperationResponse>(body);
        assert_eq!(response.status, "queued");
        let command = fixture
            .state
            .commands()
            .get_for_tenant(
                fixture.tenant_id,
                CommandId::parse(&response.command_id).unwrap(),
            )
            .await
            .unwrap()
            .unwrap();
        let payload: PrinterOperationPayload = serde_json::from_str(&command.payload_json).unwrap();
        assert_eq!(
            payload.operation,
            PrinterOperationKind::GcodeLine {
                param: param.to_owned(),
            }
        );
        assert_eq!(
            fixture.state.commands().count().await.unwrap(),
            expected_count
        );
    }
}

#[tokio::test]
async fn plugin_gcode_line_rejects_non_string_missing_and_extra_fields_without_insert() {
    let fixture = operation_fixture("plugin-gcode-line-invalid").await;
    let mut cases = vec![
        serde_json::json!({"action": "gcode_line"}),
        serde_json::json!({"action": "gcode_line", "param": null}),
        serde_json::json!({"action": "gcode_line", "param": true}),
        serde_json::json!({"action": "gcode_line", "param": 1}),
        serde_json::json!({"action": "gcode_line", "param": []}),
        serde_json::json!({"action": "gcode_line", "param": {}}),
        serde_json::json!({"action": "gcode_line", "param": "M620 C1", "unexpected": true}),
    ];
    for (field, value) in [
        ("speed_mode", serde_json::json!(1)),
        ("axes", serde_json::json!([])),
        ("movements", serde_json::json!([])),
        ("feedrate_mm_per_min", serde_json::json!(1)),
        ("temperature_celsius", serde_json::json!(1)),
        ("wait", serde_json::json!(false)),
        ("ams_id", serde_json::json!(0)),
        ("slot_id", serde_json::json!(0)),
        ("global_tray_id", serde_json::json!(0)),
        ("external_id", serde_json::json!("external")),
        ("extruder_id", serde_json::json!(0)),
        ("light_on", serde_json::json!(false)),
        ("error_action", serde_json::json!("resume")),
        ("print_error", serde_json::json!(1)),
        ("printer_job_id", serde_json::json!("job")),
        ("sequence_id", serde_json::json!(1)),
        ("error_generation", serde_json::json!(1)),
    ] {
        let mut body = serde_json::json!({"action": "gcode_line", "param": "M620 C1"});
        body.as_object_mut()
            .unwrap()
            .insert(field.to_owned(), value);
        cases.push(body);
    }
    for required_device_features in [
        serde_json::Value::Null,
        serde_json::json!([]),
        serde_json::json!(["bambu_mqtt_homing"]),
        serde_json::json!(["bambu_mqtt_axis_control"]),
        serde_json::json!(["unspecified"]),
    ] {
        cases.push(serde_json::json!({
            "action": "gcode_line",
            "param": "M620 C1",
            "required_device_features": required_device_features,
        }));
    }

    for request in cases {
        let (status, body) = request_as(
            fixture.app.clone(),
            Method::POST,
            &fixture.uri,
            Some(request),
            &fixture.token,
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            decode::<OperationErrorResponse>(body).error,
            "invalid_printer_control"
        );
        assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
    }
    assert!(
        fixture
            .state
            .audit_events()
            .list_for_tenant(fixture.tenant_id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn plugin_gcode_line_obeys_existing_router_body_limit_without_partial_insert() {
    const ROUTER_BODY_LIMIT: usize = 64 * 1024;
    let fixture = operation_fixture("plugin-gcode-line-body-limit").await;
    let below = gcode_line_body_with_serialized_len(ROUTER_BODY_LIMIT - 1);
    let above = gcode_line_body_with_serialized_len(ROUTER_BODY_LIMIT + 1);

    let (status, body) = request_as(
        fixture.app.clone(),
        Method::POST,
        &fixture.uri,
        Some(below),
        &fixture.token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode::<OperationResponse>(body).status, "queued");
    assert_eq!(fixture.state.commands().count().await.unwrap(), 1);

    let (status, body) = request_as(
        fixture.app.clone(),
        Method::POST,
        &fixture.uri,
        Some(above),
        &fixture.token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        decode::<OperationErrorResponse>(body).error,
        "invalid_printer_control"
    );
    assert_eq!(fixture.state.commands().count().await.unwrap(), 1);
}

#[tokio::test]
async fn required_device_features_accept_exact_modern_home_and_signed_moves() {
    let fixture = operation_fixture("plugin-required-device-features-valid").await;
    let cases = [
        serde_json::json!({
            "action": "home",
            "axes": [],
            "required_device_features": ["bambu_mqtt_homing"]
        }),
        serde_json::json!({
            "action": "move_axes",
            "movements": [{"axis": "x", "delta_mm": -1.0}],
            "required_device_features": ["bambu_mqtt_axis_control"]
        }),
        serde_json::json!({
            "action": "move_axes",
            "movements": [{"axis": "x", "delta_mm": 1.0}],
            "required_device_features": ["bambu_mqtt_axis_control"]
        }),
        serde_json::json!({
            "action": "move_axes",
            "movements": [{"axis": "z", "delta_mm": -10.0}],
            "required_device_features": ["bambu_mqtt_axis_control"]
        }),
        serde_json::json!({
            "action": "move_axes",
            "movements": [{"axis": "z", "delta_mm": 10.0}],
            "required_device_features": ["bambu_mqtt_axis_control"]
        }),
    ];

    for request in cases {
        let expected_features = request["required_device_features"].clone();
        let (status, body) = request_as(
            fixture.app.clone(),
            Method::POST,
            &fixture.uri,
            Some(request),
            &fixture.token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let response = decode::<OperationResponse>(body);
        let command = fixture
            .state
            .commands()
            .get_for_tenant(
                fixture.tenant_id,
                CommandId::parse(&response.command_id).unwrap(),
            )
            .await
            .unwrap()
            .unwrap();
        let payload: Value = serde_json::from_str(&command.payload_json).unwrap();
        assert_eq!(
            payload["operation"]["required_device_features"],
            expected_features
        );
    }

    let events = fixture
        .state
        .audit_events()
        .list_for_tenant(fixture.tenant_id)
        .await
        .unwrap();
    let feature_lists = events
        .iter()
        .filter(|event| event.action == "printer.dispatch_control")
        .map(|event| serde_json::from_str::<Value>(&event.metadata_json).unwrap())
        .map(|metadata| metadata["required_device_features"].clone())
        .collect::<Vec<_>>();
    assert_eq!(feature_lists.len(), 5);
    assert_eq!(feature_lists[0], serde_json::json!(["bambu_mqtt_homing"]));
    assert!(
        feature_lists[1..]
            .iter()
            .all(|features| *features == serde_json::json!(["bambu_mqtt_axis_control"]))
    );
}

#[tokio::test]
async fn required_device_features_reject_invalid_or_mismatched_semantics() {
    let fixture = operation_fixture("plugin-required-device-features-invalid").await;
    let cases = [
        serde_json::json!({
            "action": "home",
            "axes": ["x"],
            "required_device_features": ["bambu_mqtt_homing"]
        }),
        serde_json::json!({
            "action": "move_axes",
            "movements": [
                {"axis": "x", "delta_mm": 1.0},
                {"axis": "y", "delta_mm": 1.0}
            ],
            "required_device_features": ["bambu_mqtt_axis_control"]
        }),
        serde_json::json!({
            "action": "move_axes",
            "movements": [{"axis": "x", "delta_mm": 2.0}],
            "required_device_features": ["bambu_mqtt_axis_control"]
        }),
        serde_json::json!({
            "action": "move_axes",
            "movements": [{"axis": "x", "delta_mm": 10.0}],
            "feedrate_mm_per_min": 6_000,
            "required_device_features": ["bambu_mqtt_axis_control"]
        }),
        serde_json::json!({
            "action": "pause",
            "required_device_features": ["bambu_mqtt_homing"]
        }),
        serde_json::json!({
            "action": "home",
            "axes": [],
            "required_device_features": ["bambu_mqtt_homing", "bambu_mqtt_homing"]
        }),
        serde_json::json!({
            "action": "home",
            "axes": [],
            "required_device_features": ["bambu_mqtt_axis_control"]
        }),
        serde_json::json!({
            "action": "move_axes",
            "movements": [{"axis": "x", "delta_mm": 1.0}],
            "required_device_features": ["bambu_mqtt_homing"]
        }),
        serde_json::json!({
            "action": "home",
            "axes": [],
            "required_device_features": ["unspecified"]
        }),
        serde_json::json!({
            "action": "home",
            "axes": [],
            "required_device_features": ["unknown"]
        }),
    ];

    for request in cases {
        let (status, _) = request_as(
            fixture.app.clone(),
            Method::POST,
            &fixture.uri,
            Some(request),
            &fixture.token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn plugin_with_both_capabilities_preserves_the_studio_nonzero_sequence_path() {
    let fixture = operation_fixture("plugin-native-both-capabilities").await;
    let (wake_sender, _) = mpsc::channel(1);
    let (command_sender, mut command_receiver) = mpsc::channel(1);
    register_session(
        &fixture,
        wake_sender,
        command_sender,
        [
            AgentCapability::HandlePrintError,
            AgentCapability::HandlePrintErrorSequenceZeroPubackOnly,
        ],
    )
    .await;

    let (status, _) = request_as(
        fixture.app.clone(),
        Method::POST,
        &fixture.uri,
        Some(native_body("resume")),
        &fixture.token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let emitted = command_receiver.recv().await.unwrap().unwrap();
    let Some(hub_command::Command::PrinterOperation(operation)) = emitted.command else {
        panic!("expected printer operation command");
    };
    let Some(printer_operation::Operation::HandlePrintError(operation)) = operation.operation
    else {
        panic!("expected handle print error operation");
    };
    assert_eq!(operation.sequence_id, 20_042);
}

#[tokio::test]
async fn plugin_native_recovery_is_single_flight_and_terminal_commands_allow_retry() {
    let fixture = operation_fixture("plugin-native-single-flight").await;
    let (wake_sender, _) = mpsc::channel(1);
    let (command_sender, _command_receiver) = mpsc::channel(4);
    register_session(
        &fixture,
        wake_sender,
        command_sender,
        [AgentCapability::HandlePrintError],
    )
    .await;

    let (status, body) = request_as(
        fixture.app.clone(),
        Method::POST,
        &fixture.uri,
        Some(native_body("resume")),
        &fixture.token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let first = decode::<OperationResponse>(body);

    let (status, body) = request_as(
        fixture.app.clone(),
        Method::POST,
        &fixture.uri,
        Some(native_body("ignore")),
        &fixture.token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        decode::<OperationErrorResponse>(body).error,
        "printer_operation_unavailable"
    );
    assert_eq!(fixture.state.commands().count().await.unwrap(), 1);

    fixture
        .state
        .commands()
        .mark_succeeded(
            CommandId::parse(&first.command_id).unwrap(),
            fixture.tenant_id,
            fixture.agent_id,
        )
        .await
        .unwrap();
    let (status, _) = request_as(
        fixture.app.clone(),
        Method::POST,
        &fixture.uri,
        Some(native_body("stop")),
        &fixture.token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fixture.state.commands().count().await.unwrap(), 2);
}

struct OperationFixture {
    state: AppState,
    app: Router,
    tenant_id: TenantId,
    agent_id: AgentId,
    printer_id: String,
    token: String,
    uri: String,
}

async fn operation_fixture(slug: &str) -> OperationFixture {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create(slug, slug).await.unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant.id,
        agent.id,
        Some("A1"),
    )
    .await
    .unwrap();
    let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), slug).await;
    let uri = format!("/api/v1/plugin/printers/{printer_id}/operations");
    OperationFixture {
        state,
        app,
        tenant_id: tenant.id,
        agent_id: agent.id,
        printer_id,
        token,
        uri,
    }
}

async fn register_session(
    fixture: &OperationFixture,
    wake_sender: mpsc::Sender<()>,
    command_sender: mpsc::Sender<Result<crate::protocol::agent::v1::HubCommand, tonic::Status>>,
    capabilities: impl IntoIterator<Item = AgentCapability>,
) {
    register_session_for_agent(
        fixture,
        fixture.agent_id,
        wake_sender,
        command_sender,
        capabilities,
    )
    .await;
}

async fn register_session_for_agent(
    fixture: &OperationFixture,
    agent_id: AgentId,
    wake_sender: mpsc::Sender<()>,
    command_sender: mpsc::Sender<Result<crate::protocol::agent::v1::HubCommand, tonic::Status>>,
    capabilities: impl IntoIterator<Item = AgentCapability>,
) {
    fixture
        .state
        .sessions()
        .register(AgentSession {
            token: SessionToken::new(),
            tenant_id: fixture.tenant_id,
            agent_id,
            name: "agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender: mpsc::channel(1).0,
            command_sender,
            capabilities: capabilities.into_iter().collect::<HashSet<_>>(),
            pending_live_commands: empty_pending_live_commands(),
            live_command_transition: Arc::new(Mutex::new(())),
        })
        .await;
}

async fn assert_unavailable(fixture: &OperationFixture) {
    let (status, body) = request_as(
        fixture.app.clone(),
        Method::POST,
        &fixture.uri,
        Some(native_body("resume")),
        &fixture.token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        decode::<OperationErrorResponse>(body).error,
        "printer_operation_unavailable"
    );
}

async fn assert_plugin_print_error_param_rejected(slug: &str, param: Value) {
    let fixture = operation_fixture(slug).await;
    let (wake_sender, _) = mpsc::channel(1);
    let (command_sender, _command_receiver) = mpsc::channel(1);
    register_session(
        &fixture,
        wake_sender,
        command_sender,
        [AgentCapability::HandlePrintError],
    )
    .await;
    let mut request = native_body("resume");
    request
        .as_object_mut()
        .unwrap()
        .insert("param".to_owned(), param);

    let (status, body) = request_as(
        fixture.app.clone(),
        Method::POST,
        &fixture.uri,
        Some(request),
        &fixture.token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        decode::<OperationErrorResponse>(body).error,
        "invalid_printer_control"
    );
    assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
    assert!(
        fixture
            .state
            .audit_events()
            .list_for_tenant(fixture.tenant_id)
            .await
            .unwrap()
            .is_empty()
    );
}

fn native_body(error_action: &str) -> Value {
    serde_json::json!({
        "action": "handle_print_error",
        "error_action": error_action,
        "print_error": 83_918_929,
        "printer_job_id": "job-7",
        "sequence_id": 20_042
    })
}

fn gcode_line_body_with_serialized_len(target_len: usize) -> Value {
    let empty = serde_json::json!({"action": "gcode_line", "param": ""});
    let overhead = empty.to_string().len();
    let body = serde_json::json!({
        "action": "gcode_line",
        "param": "x".repeat(target_len - overhead),
    });
    assert_eq!(body.to_string().len(), target_len);
    body
}
