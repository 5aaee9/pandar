use std::{collections::HashSet, sync::Arc, time::Duration};

use pandar_core::{CommandId, CommandStatus};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, IntoActiveModel};
use tokio::sync::{Mutex, mpsc};

use super::*;
use crate::{
    protocol::agent::v1::{
        AgentCapability, PrintErrorAction as ProtoPrintErrorAction, hub_command, printer_operation,
    },
    repositories::{PrintErrorAction, PrinterOperationKind, PrinterOperationPayload, UserRole},
    sessions::{AgentSession, SessionToken, empty_pending_live_commands},
};

const ERROR_GENERATION: u64 = 9;
const BUILD_PLATE_MISMATCH: u32 = 83_918_929;
const BUILD_PLATE_MARKER_NOT_DETECTED: u32 = 83_918_946;
const BUILD_PLATE_OFFSET: u32 = 83_918_988;

mod single_flight;

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
async fn tenant_recovery_catalog_is_exactly_six_families_and_three_actions() {
    for family in ["093", "094", "20P", "22E", "239", "31B"] {
        for (action, expected) in [
            ("resume", PrintErrorAction::Resume),
            ("ignore", PrintErrorAction::Ignore),
            ("stop", PrintErrorAction::Stop),
        ] {
            let slug = format!("catalog-{}-{action}", family.to_ascii_lowercase());
            let fixture = RecoveryFixture::new(
                &slug,
                &format!("{family}123456789"),
                [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
            )
            .await;

            let (status, body) = fixture.request(action, ERROR_GENERATION).await;

            assert_eq!(status, StatusCode::OK, "{family} {action}: {body}");
            let response = decode::<CommandResponse>(body);
            let command = fixture
                .state
                .commands()
                .get_for_tenant(fixture.tenant_id, CommandId::parse(&response.id).unwrap())
                .await
                .unwrap()
                .unwrap();
            let payload: PrinterOperationPayload =
                serde_json::from_str(&command.payload_json).unwrap();
            assert!(matches!(
                payload.operation,
                PrinterOperationKind::HandlePrintError {
                    error_action,
                    print_error: BUILD_PLATE_MISMATCH,
                    sequence_id: 0,
                    ..
                } if error_action == expected
            ));
        }
    }

    for serial in ["26A123456789", "XYZ123456789", "20"] {
        let slug = format!("catalog-miss-{}", serial.to_ascii_lowercase());
        let fixture = RecoveryFixture::new(
            &slug,
            serial,
            [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
        )
        .await;

        let (status, body) = fixture.request("stop", ERROR_GENERATION).await;

        assert_unavailable(status, body);
        assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
    }
}

#[tokio::test]
async fn tenant_recovery_uses_native_build_plate_marker_actions_and_error_code() {
    for (action, expected) in [
        ("ignore", PrintErrorAction::Ignore),
        ("resume", PrintErrorAction::Resume),
    ] {
        let fixture = RecoveryFixture::new(
            &format!("marker-{action}"),
            "20P123456789",
            [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
        )
        .await;
        mutate_printer(
            &fixture,
            RecoveryMutation::PrintError(Some(BUILD_PLATE_MARKER_NOT_DETECTED as i32)),
        )
        .await;

        let (status, body) = fixture.request(action, ERROR_GENERATION).await;

        assert_eq!(status, StatusCode::OK, "{action}: {body}");
        let response = decode::<CommandResponse>(body);
        let command = fixture
            .state
            .commands()
            .get_for_tenant(fixture.tenant_id, CommandId::parse(&response.id).unwrap())
            .await
            .unwrap()
            .unwrap();
        let payload: PrinterOperationPayload = serde_json::from_str(&command.payload_json).unwrap();
        assert!(matches!(
            payload.operation,
            PrinterOperationKind::HandlePrintError {
                error_action,
                print_error: BUILD_PLATE_MARKER_NOT_DETECTED,
                sequence_id: 0,
                ..
            } if error_action == expected
        ));
    }

    let fixture = RecoveryFixture::new(
        "marker-stop-rejected",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    mutate_printer(
        &fixture,
        RecoveryMutation::PrintError(Some(BUILD_PLATE_MARKER_NOT_DETECTED as i32)),
    )
    .await;

    let (status, body) = fixture.request("stop", ERROR_GENERATION).await;

    assert_unavailable(status, body);
    assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn tenant_recovery_preserves_additional_server_owned_plate_error() {
    let fixture = RecoveryFixture::new(
        "plate-offset-ignore",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    mutate_printer(
        &fixture,
        RecoveryMutation::PrintError(Some(BUILD_PLATE_OFFSET as i32)),
    )
    .await;

    let (status, body) = fixture.request("ignore", ERROR_GENERATION).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let response = decode::<CommandResponse>(body);
    let command = fixture
        .state
        .commands()
        .get_for_tenant(fixture.tenant_id, CommandId::parse(&response.id).unwrap())
        .await
        .unwrap()
        .unwrap();
    let payload: PrinterOperationPayload = serde_json::from_str(&command.payload_json).unwrap();
    assert!(matches!(
        payload.operation,
        PrinterOperationKind::HandlePrintError {
            error_action: PrintErrorAction::Ignore,
            print_error: BUILD_PLATE_OFFSET,
            sequence_id: 0,
            ..
        }
    ));
}

#[tokio::test]
async fn tenant_recovery_parser_rejects_transport_state_and_cross_operation_fields() {
    let fixture = RecoveryFixture::new(
        "tenant-native-parser",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    let invalid = [
        serde_json::json!({"error_action":"resume","error_generation":9}),
        serde_json::json!({"action":null,"error_action":"resume","error_generation":9}),
        serde_json::json!({"action":"handle_print_error","error_generation":9}),
        serde_json::json!({"action":"handle_print_error","error_action":null,"error_generation":9}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume"}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":null}),
        serde_json::json!({"action":"handle_print_error","error_action":"retry","error_generation":9}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":-1}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"print_error":BUILD_PLATE_MISMATCH}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"printer_job_id":"forged"}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"job_id":"forged"}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"sequence_id":0}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"job_attr":0}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"job_state":0}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"task_generation":9}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"speed_mode":1}),
        serde_json::json!({"action":"handle_print_error","error_action":"resume","error_generation":9,"unexpected":true}),
        serde_json::json!({"action":"pause","error_generation":9}),
    ];

    for body in invalid {
        let (status, body) = request_as(
            fixture.app.clone(),
            Method::POST,
            &fixture.uri,
            Some(body),
            &fixture.token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            decode::<ErrorResponse>(body).error,
            "invalid_printer_control"
        );
    }
    assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
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

#[tokio::test]
async fn tenant_recovery_revalidates_every_authoritative_error_and_state_guard() {
    let cases = [
        ("cleared", RecoveryMutation::PrintError(None)),
        (
            "different-error",
            RecoveryMutation::PrintError(Some(BUILD_PLATE_MISMATCH as i32 + 1)),
        ),
        (
            "generation",
            RecoveryMutation::ErrorGeneration(ERROR_GENERATION as i64 - 1),
        ),
        (
            "task-marker",
            RecoveryMutation::ErrorTaskGeneration(Some(ERROR_GENERATION as i64 - 1)),
        ),
        (
            "session-marker",
            RecoveryMutation::ErrorSession(Some("other-session")),
        ),
        ("receive-marker", RecoveryMutation::ErrorReceivedAt(None)),
        ("native-missing", RecoveryMutation::GcodeState(None)),
        (
            "native-unknown",
            RecoveryMutation::GcodeState(Some("UNKNOWN")),
        ),
        ("native-idle", RecoveryMutation::GcodeState(Some("IDLE"))),
        (
            "native-finish",
            RecoveryMutation::GcodeState(Some("FINISH")),
        ),
        (
            "native-failed",
            RecoveryMutation::GcodeState(Some("FAILED")),
        ),
        ("coarse-idle", RecoveryMutation::CoarseState("IDLE")),
        ("coarse-offline", RecoveryMutation::CoarseState("offline")),
        ("coarse-failed", RecoveryMutation::CoarseState("FAILED")),
        ("job-attr-missing", RecoveryMutation::JobAttr(None)),
        ("job-state-unsafe", RecoveryMutation::JobAttr(Some(0x20))),
    ];

    for (case, mutation) in cases {
        let fixture = RecoveryFixture::new(
            &format!("guard-{case}"),
            "20P123456789",
            [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
        )
        .await;
        mutate_printer(&fixture, mutation).await;

        let (status, body) = fixture.request("resume", ERROR_GENERATION).await;

        assert_unavailable(status, body);
        assert_eq!(
            fixture.state.commands().count().await.unwrap(),
            0,
            "guard case {case} persisted a command"
        );
    }

    let fixture = RecoveryFixture::new(
        "guard-stale-client-generation",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    let (status, body) = fixture.request("resume", ERROR_GENERATION - 1).await;
    assert_unavailable(status, body);
    assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn tenant_recovery_accepts_only_the_four_native_active_states() {
    for native_state in ["PREPARE", "SLICING", "RUNNING", "PAUSE"] {
        let fixture = RecoveryFixture::new(
            &format!("native-state-{}", native_state.to_ascii_lowercase()),
            "20P123456789",
            [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
        )
        .await;
        mutate_printer(&fixture, RecoveryMutation::GcodeState(Some(native_state))).await;

        let (status, body) = fixture.request("resume", ERROR_GENERATION).await;

        assert_eq!(status, StatusCode::OK, "{native_state}: {body}");
    }
}

#[tokio::test]
async fn tenant_recovery_derives_job_state_bits_and_stop_does_not_use_the_guard() {
    for (slug, action, job_attr) in [
        ("job-state-zero", "ignore", Some(0x00)),
        ("job-state-one", "resume", Some(0x1f)),
        ("stop-job-state-unknown", "stop", None),
        ("stop-job-state-unsafe", "stop", Some(0xf0)),
    ] {
        let fixture = RecoveryFixture::new(
            slug,
            "20P123456789",
            [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
        )
        .await;
        mutate_printer(&fixture, RecoveryMutation::JobAttr(job_attr)).await;

        let (status, body) = fixture.request(action, ERROR_GENERATION).await;

        assert_eq!(status, StatusCode::OK, "{slug}: {body}");
    }
}

#[tokio::test]
async fn tenant_recovery_preserves_explicit_empty_and_unknown_job_ids_as_empty() {
    for (slug, job_id) in [("empty-job", Some("")), ("unknown-job", None)] {
        let fixture = RecoveryFixture::new(
            slug,
            "20P123456789",
            [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
        )
        .await;
        mutate_printer(&fixture, RecoveryMutation::PrinterJobId(job_id)).await;

        let (status, body) = fixture.request("stop", ERROR_GENERATION).await;

        assert_eq!(status, StatusCode::OK, "{slug}: {body}");
        let response = decode::<CommandResponse>(body);
        let command = fixture
            .state
            .commands()
            .get_for_tenant(fixture.tenant_id, CommandId::parse(&response.id).unwrap())
            .await
            .unwrap()
            .unwrap();
        let payload: PrinterOperationPayload = serde_json::from_str(&command.payload_json).unwrap();
        assert!(matches!(
            payload.operation,
            PrinterOperationKind::HandlePrintError { printer_job_id, .. } if printer_job_id.is_empty()
        ));
    }
}

#[tokio::test]
async fn tenant_recovery_rejects_offline_or_replaced_persisted_agent_session() {
    let offline = RecoveryFixture::new(
        "agent-offline",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    offline
        .state
        .agents()
        .mark_offline_if_current(
            offline.tenant_id,
            offline.agent_id,
            &offline.session_id,
            &pandar_core::created_at_now(),
        )
        .await
        .unwrap();
    let (status, body) = offline.request("resume", ERROR_GENERATION).await;
    assert_unavailable(status, body);

    let replaced = RecoveryFixture::new(
        "agent-replaced",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    replaced
        .state
        .agents()
        .claim_online_session(
            replaced.tenant_id,
            replaced.agent_id,
            &SessionToken::new().persisted_id(),
            "replacement",
            &pandar_core::created_at_now(),
        )
        .await
        .unwrap();
    let (status, body) = replaced.request("resume", ERROR_GENERATION).await;
    assert_unavailable(status, body);
}

#[tokio::test]
async fn tenant_recovery_revalidates_printer_state_after_the_route_owner_read() {
    let fixture = RecoveryFixture::new(
        "tenant-native-state-race",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    let pause =
        crate::repositories::printer_operation_ownership_pause::install(&fixture.printer_id);
    let request = tokio::spawn({
        let app = fixture.app.clone();
        let uri = fixture.uri.clone();
        let token = fixture.token.clone();
        async move {
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
    let resume = pause.wait_until_reached().await.unwrap();
    mutate_printer(&fixture, RecoveryMutation::PrintError(None)).await;
    resume.send(()).unwrap();

    let (status, body) = request.await.unwrap();

    assert_unavailable(status, body);
    assert_eq!(fixture.state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn agent_replacement_waits_until_web_recovery_is_persisted_and_enqueued() {
    let mut fixture = RecoveryFixture::new(
        "tenant-native-lease-race",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    let mut pause = crate::repositories::current_transaction_pause::install(&fixture.session_id);
    let request = tokio::spawn({
        let app = fixture.app.clone();
        let uri = fixture.uri.clone();
        let token = fixture.token.clone();
        async move {
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
    pause.wait_until_reached().await;

    let (lease_acquired_sender, mut lease_acquired_receiver) = tokio::sync::oneshot::channel();
    let replacement = tokio::spawn({
        let state = fixture.state.clone();
        let tenant_id = fixture.tenant_id;
        let agent_id = fixture.agent_id;
        async move {
            let _lease = state.sessions().transition_lease(agent_id).await;
            let _ = lease_acquired_sender.send(());
            state
                .agents()
                .claim_online_session(
                    tenant_id,
                    agent_id,
                    &SessionToken::new().persisted_id(),
                    "replacement",
                    &pandar_core::created_at_now(),
                )
                .await
                .unwrap();
        }
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut lease_acquired_receiver)
            .await
            .is_err(),
        "replacement acquired the transition lease while recovery owned it"
    );

    pause.resume();
    let (status, body) = request.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    tokio::time::timeout(Duration::from_secs(1), &mut lease_acquired_receiver)
        .await
        .unwrap()
        .unwrap();
    replacement.await.unwrap();
    assert!(fixture.command_receiver.recv().await.unwrap().is_ok());
}

#[tokio::test]
async fn concurrent_tenant_recoveries_persist_and_dispatch_only_one_command() {
    let fixture = RecoveryFixture::new_file(
        "tenant-native-race",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;

    let (left, right) = tokio::join!(
        fixture.request("resume", ERROR_GENERATION),
        fixture.request("ignore", ERROR_GENERATION),
    );

    let statuses = [left.0, right.0];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::BAD_REQUEST)
            .count(),
        1
    );
    let unavailable = if left.0 == StatusCode::BAD_REQUEST {
        left.1
    } else {
        right.1
    };
    assert_eq!(
        decode::<ErrorResponse>(unavailable).error,
        "printer_operation_unavailable"
    );
    assert_eq!(fixture.state.commands().count().await.unwrap(), 1);
}

#[tokio::test]
async fn studio_native_recovery_blocks_web_until_the_command_is_terminal() {
    let fixture = RecoveryFixture::new(
        "tenant-native-studio-overlap",
        "20P123456789",
        [
            AgentCapability::HandlePrintError,
            AgentCapability::HandlePrintErrorSequenceZeroPubackOnly,
        ],
    )
    .await;
    let studio = fixture
        .state
        .commands()
        .create_printer_operation_sent_with_audit(
            fixture.tenant_id,
            &fixture.printer_id,
            fixture.agent_id,
            PrinterOperationKind::HandlePrintError {
                error_action: PrintErrorAction::Resume,
                print_error: BUILD_PLATE_MISMATCH,
                printer_job_id: "job-7".to_owned(),
                sequence_id: 20_042,
            },
            crate::repositories::AuditActor::tenant_token(
                None,
                "studio-overlap",
                vec!["plugin:studio"],
            ),
        )
        .await
        .unwrap();

    let (status, body) = fixture.request("stop", ERROR_GENERATION).await;
    assert_unavailable(status, body);
    assert_eq!(fixture.state.commands().count().await.unwrap(), 1);

    fixture
        .state
        .commands()
        .mark_succeeded(studio.id, fixture.tenant_id, fixture.agent_id)
        .await
        .unwrap();
    let (status, body) = fixture.request("stop", ERROR_GENERATION).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(fixture.state.commands().count().await.unwrap(), 2);
}

#[tokio::test]
async fn tenant_recovery_dispatch_failure_marks_the_sent_command_failed() {
    let mut fixture = RecoveryFixture::new(
        "tenant-native-send-failure",
        "20P123456789",
        [AgentCapability::HandlePrintErrorSequenceZeroPubackOnly],
    )
    .await;
    fixture.command_receiver.close();

    let (status, body) = fixture.request("resume", ERROR_GENERATION).await;

    assert_unavailable(status, body);
    let command = crate::entities::commands::Entity::find()
        .one(&fixture.state.database().sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(command.status, "failed");
    assert!(command.error.unwrap().contains("ChannelClosed"));
}

struct RecoveryFixture {
    state: AppState,
    app: Router,
    tenant_id: TenantId,
    agent_id: AgentId,
    printer_id: String,
    token: String,
    plugin_token: String,
    session_id: String,
    uri: String,
    plugin_uri: String,
    command_receiver: mpsc::Receiver<Result<crate::protocol::agent::v1::HubCommand, tonic::Status>>,
    wake_receiver: mpsc::Receiver<()>,
}

impl RecoveryFixture {
    async fn new(
        slug: &str,
        serial_number: &str,
        capabilities: impl IntoIterator<Item = AgentCapability>,
    ) -> Self {
        let state = state().await;
        Self::with_state(state, slug, serial_number, capabilities).await
    }

    async fn new_file(
        slug: &str,
        serial_number: &str,
        capabilities: impl IntoIterator<Item = AgentCapability>,
    ) -> Self {
        let state = AppState::file_sqlite_for_tests().await.unwrap();
        Self::with_state(state, slug, serial_number, capabilities).await
    }

    async fn with_state(
        state: AppState,
        slug: &str,
        serial_number: &str,
        capabilities: impl IntoIterator<Item = AgentCapability>,
    ) -> Self {
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
        let session_token = SessionToken::new();
        let session_id = session_token.persisted_id();
        let now = pandar_core::created_at_now();
        state
            .agents()
            .claim_online_session(tenant.id, agent.id, &session_id, "test", &now)
            .await
            .unwrap();
        set_recovery_state(
            &state,
            &printer_id,
            serial_number,
            &session_id,
            Some(0x10),
            Some("job-7"),
        )
        .await;
        let (wake_sender, wake_receiver) = mpsc::channel(1);
        let (command_sender, command_receiver) = mpsc::channel(2);
        state
            .sessions()
            .register(AgentSession {
                token: session_token,
                tenant_id: tenant.id,
                agent_id: agent.id,
                name: "agent".to_owned(),
                version: "test".to_owned(),
                connected_at: now.clone(),
                last_heartbeat_at: now,
                wake_sender,
                close_sender: mpsc::channel(1).0,
                command_sender,
                capabilities: capabilities.into_iter().collect::<HashSet<_>>(),
                pending_live_commands: empty_pending_live_commands(),
                live_command_transition: Arc::new(Mutex::new(())),
            })
            .await;
        let token = auth_token_for_role(
            &state,
            &tenant.id.to_string(),
            UserRole::Operator,
            &format!("{slug}-token"),
        )
        .await;
        let plugin_token =
            plugin_studio_tenant_token(&state, &tenant.id.to_string(), &format!("{slug}-plugin"))
                .await;
        let uri = format!(
            "/api/v1/tenants/{}/printers/{printer_id}/controls",
            tenant.id
        );
        let plugin_uri = format!("/api/v1/plugin/printers/{printer_id}/operations");
        Self {
            state,
            app,
            tenant_id: tenant.id,
            agent_id: agent.id,
            printer_id,
            token,
            plugin_token,
            session_id,
            uri,
            plugin_uri,
            command_receiver,
            wake_receiver,
        }
    }

    async fn request(&self, action: &str, generation: u64) -> (StatusCode, Value) {
        request_as(
            self.app.clone(),
            Method::POST,
            &self.uri,
            Some(recovery_body(action, generation)),
            &self.token,
        )
        .await
    }

    async fn plugin_request(&self, action: &str) -> (StatusCode, Value) {
        request_as(
            self.app.clone(),
            Method::POST,
            &self.plugin_uri,
            Some(plugin_recovery_body(action)),
            &self.plugin_token,
        )
        .await
    }
}

enum RecoveryMutation {
    PrintError(Option<i32>),
    ErrorGeneration(i64),
    ErrorTaskGeneration(Option<i64>),
    ErrorSession(Option<&'static str>),
    ErrorReceivedAt(Option<&'static str>),
    GcodeState(Option<&'static str>),
    CoarseState(&'static str),
    JobAttr(Option<i64>),
    PrinterJobId(Option<&'static str>),
}

async fn mutate_printer(fixture: &RecoveryFixture, mutation: RecoveryMutation) {
    let printer = crate::entities::printers::Entity::find_by_id(&fixture.printer_id)
        .one(&fixture.state.database().sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    let mut active = printer.into_active_model();
    match mutation {
        RecoveryMutation::PrintError(value) => active.print_error = Set(value),
        RecoveryMutation::ErrorGeneration(value) => active.print_error_generation = Set(value),
        RecoveryMutation::ErrorTaskGeneration(value) => {
            active.print_error_task_generation = Set(value)
        }
        RecoveryMutation::ErrorSession(value) => {
            active.print_error_session_id = Set(value.map(str::to_owned))
        }
        RecoveryMutation::ErrorReceivedAt(value) => {
            active.print_error_received_at = Set(value.map(str::to_owned))
        }
        RecoveryMutation::GcodeState(value) => {
            active.print_gcode_state = Set(value.map(str::to_owned))
        }
        RecoveryMutation::CoarseState(value) => active.status = Set(value.to_owned()),
        RecoveryMutation::JobAttr(value) => active.print_job_attr = Set(value),
        RecoveryMutation::PrinterJobId(value) => {
            active.print_job_id = Set(value.map(str::to_owned))
        }
    }
    active
        .update(&fixture.state.database().sea_orm_connection())
        .await
        .unwrap();
}

async fn set_recovery_state(
    state: &AppState,
    printer_id: &str,
    serial_number: &str,
    session_id: &str,
    job_attr: Option<i64>,
    printer_job_id: Option<&str>,
) {
    let printer = crate::entities::printers::Entity::find_by_id(printer_id)
        .one(&state.database().sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    let mut active = printer.into_active_model();
    active.serial_number = Set(serial_number.to_owned());
    active.status = Set("RUNNING".to_owned());
    active.print_task_generation = Set(ERROR_GENERATION as i64);
    active.print_error_generation = Set(ERROR_GENERATION as i64);
    active.print_job_attr = Set(job_attr);
    active.print_error_task_generation = Set(Some(ERROR_GENERATION as i64));
    active.print_error_session_id = Set(Some(session_id.to_owned()));
    active.print_error_received_at = Set(Some("2026-07-10T00:00:00Z".to_owned()));
    active.print_gcode_state = Set(Some("PAUSE".to_owned()));
    active.print_error = Set(Some(BUILD_PLATE_MISMATCH as i32));
    active.print_job_id = Set(printer_job_id.map(str::to_owned));
    active
        .update(&state.database().sea_orm_connection())
        .await
        .unwrap();
}

fn recovery_body(action: &str, generation: u64) -> Value {
    web_print_error_body(action, generation).unwrap()
}

fn plugin_recovery_body(action: &str) -> Value {
    serde_json::json!({
        "action": "handle_print_error",
        "error_action": action,
        "print_error": BUILD_PLATE_MISMATCH,
        "printer_job_id": "job-7",
        "sequence_id": 20_042
    })
}

fn assert_unavailable(status: StatusCode, body: Value) {
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        decode::<ErrorResponse>(body).error,
        "printer_operation_unavailable"
    );
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct WebPrintErrorAuditMetadata {
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
