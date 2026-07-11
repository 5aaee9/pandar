use pandar_core::{AgentId, CommandRecord, CommandStatus, TenantId};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, IntoActiveModel};
use serde::Deserialize;

use super::*;
use crate::repositories::{
    AgentRepository, AuditActor, PrintErrorAction, PrinterOperationKind, PrinterOperationPayload,
    WebPrintErrorRecovery,
};

#[tokio::test]
async fn handle_print_error_sent_persists_exact_payload_and_flat_audit() {
    let (_, tenant_id, agent_id, printer_id, commands, audit) = setup().await;
    let serial_number = format!("serial-{printer_id}");

    for (action, action_name, sequence_id) in [
        (PrintErrorAction::Resume, "resume", 20_042),
        (PrintErrorAction::Ignore, "ignore", 20_043),
        (PrintErrorAction::Stop, "stop", 20_044),
    ] {
        let command = sent_native(
            &commands,
            tenant_id,
            agent_id,
            &printer_id,
            action,
            83_918_929,
            sequence_id,
        )
        .await;

        assert_eq!(command.kind, "printer_operation");
        assert_eq!(command.status, CommandStatus::Sent);
        assert_eq!(command.agent_id, agent_id);
        assert_eq!(command.printer_id.as_deref(), Some(printer_id.as_str()));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&command.payload_json).unwrap(),
            serde_json::json!({
                "printer_id": printer_id,
                "serial_number": serial_number,
                "operation": {
                    "type": "handle_print_error",
                    "error_action": action_name,
                    "print_error": 83_918_929,
                    "printer_job_id": "job-7",
                    "sequence_id": sequence_id
                }
            })
        );
        let payload: PrinterOperationPayload = serde_json::from_str(&command.payload_json).unwrap();
        assert_eq!(
            payload.operation,
            native_operation(action, 83_918_929, sequence_id)
        );
        commands
            .mark_succeeded(command.id, tenant_id, agent_id)
            .await
            .unwrap();
    }

    assert!(
        commands
            .next_queued_for_agent(tenant_id, agent_id)
            .await
            .unwrap()
            .is_none()
    );
    let events = audit.list_for_tenant(tenant_id).await.unwrap();
    assert_eq!(events.len(), 3);
    for (action, sequence_id) in [
        (PrintErrorAction::Resume, 20_042),
        (PrintErrorAction::Ignore, 20_043),
        (PrintErrorAction::Stop, 20_044),
    ] {
        let event = events
            .iter()
            .find(|event| {
                serde_json::from_str::<TestPrintErrorAuditMetadata>(&event.metadata_json)
                    .is_ok_and(|metadata| metadata.error_action == action)
            })
            .expect("print error audit event");
        assert_eq!(event.action, "printer.dispatch_control");
        assert_eq!(event.target_type, "printer");
        assert_eq!(event.target_id.as_deref(), Some(printer_id.as_str()));
        assert_eq!(
            serde_json::from_str::<TestPrintErrorAuditMetadata>(&event.metadata_json).unwrap(),
            TestPrintErrorAuditMetadata {
                agent_id: agent_id.to_string(),
                serial_number: serial_number.clone(),
                action: "handle_print_error".to_owned(),
                error_action: action,
                print_error: 83_918_929,
                printer_job_id: "job-7".to_owned(),
                sequence_id,
                tenant_token_id: "repository-native-print-error".to_owned(),
                tenant_token_scopes: vec!["*".to_owned()],
            }
        );
    }
}

#[tokio::test]
async fn handle_print_error_queued_constructor_rejects_without_persisting() {
    let (_, tenant_id, _, printer_id, commands, audit) = setup().await;

    let err = commands
        .enqueue_printer_operation_with_audit(
            tenant_id,
            &printer_id,
            native_operation(PrintErrorAction::Resume, 83_918_929, 20_042),
            native_audit_actor(),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::InvalidPrinterControl));
    assert_eq!(commands.count().await.unwrap(), 0);
    assert!(audit.list_for_tenant(tenant_id).await.unwrap().is_empty());
}

#[tokio::test]
async fn handle_print_error_validation_accepts_i32_max_and_rejects_outside() {
    let (_, tenant_id, agent_id, printer_id, commands, audit) = setup().await;

    let accepted = sent_native(
        &commands,
        tenant_id,
        agent_id,
        &printer_id,
        PrintErrorAction::Resume,
        i32::MAX as u32,
        20_042,
    )
    .await;
    assert_eq!(accepted.status, CommandStatus::Sent);

    for print_error in [0, i32::MAX as u32 + 1] {
        let err = commands
            .create_printer_operation_sent_with_audit(
                tenant_id,
                &printer_id,
                agent_id,
                native_operation(PrintErrorAction::Resume, print_error, 20_043),
                native_audit_actor(),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, RepositoryError::InvalidPrinterControl));
    }

    assert_eq!(commands.count().await.unwrap(), 1);
    assert_eq!(audit.list_for_tenant(tenant_id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn web_print_error_uses_server_state_and_shares_native_single_flight() {
    let (database, tenant_id, agent_id, printer_id, commands, _) = setup().await;
    let session_id = "repository-web-recovery-session";
    AgentRepository::new(database.clone())
        .claim_online_session(
            tenant_id,
            agent_id,
            session_id,
            "test",
            "2026-07-10T00:00:00Z",
        )
        .await
        .unwrap();
    seed_web_recovery_state(&database, &printer_id, session_id).await;
    let ordinary = commands
        .enqueue_printer_operation_with_audit(
            tenant_id,
            &printer_id,
            PrinterOperationKind::Pause,
            native_audit_actor(),
        )
        .await
        .unwrap();
    commands
        .mark_sent(ordinary.id, tenant_id, agent_id)
        .await
        .unwrap();

    let first = commands
        .create_web_print_error_sent_with_audit(
            tenant_id,
            &printer_id,
            web_recovery_input(PrintErrorAction::Resume, agent_id, session_id),
            native_audit_actor(),
        )
        .await
        .unwrap();

    assert_eq!(first.command.status, CommandStatus::Sent);
    assert_eq!(first.serial_number, "20P123456789");
    assert_eq!(
        first.operation,
        PrinterOperationKind::HandlePrintError {
            error_action: PrintErrorAction::Resume,
            print_error: 83_918_929,
            printer_job_id: "job-7".to_owned(),
            sequence_id: 0,
        }
    );
    let error = commands
        .create_web_print_error_sent_with_audit(
            tenant_id,
            &printer_id,
            web_recovery_input(PrintErrorAction::Ignore, agent_id, session_id),
            native_audit_actor(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, RepositoryError::PrinterControlUnavailable));
    commands
        .mark_acknowledged(first.command.id, tenant_id, agent_id)
        .await
        .unwrap();
    let error = commands
        .create_web_print_error_sent_with_audit(
            tenant_id,
            &printer_id,
            web_recovery_input(PrintErrorAction::Stop, agent_id, session_id),
            native_audit_actor(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, RepositoryError::PrinterControlUnavailable));
    commands
        .mark_failed(first.command.id, tenant_id, agent_id, "transport failed")
        .await
        .unwrap();

    let retry = commands
        .create_web_print_error_sent_with_audit(
            tenant_id,
            &printer_id,
            web_recovery_input(PrintErrorAction::Stop, agent_id, session_id),
            native_audit_actor(),
        )
        .await
        .unwrap();
    assert_eq!(retry.command.status, CommandStatus::Sent);
    assert_eq!(commands.count().await.unwrap(), 3);
}

#[tokio::test]
async fn handle_print_error_stale_recovery_fails_only_unowned_typed_live_candidates() {
    let (database, tenant_id, agent_id, printer_id, commands, _) = setup().await;
    let old_owned = sent_native(
        &commands,
        tenant_id,
        agent_id,
        &printer_id,
        PrintErrorAction::Resume,
        83_918_929,
        20_042,
    )
    .await;
    let old_unowned_sent_printer = additional_printer(&database, tenant_id, agent_id).await;
    let old_unowned_sent = sent_native(
        &commands,
        tenant_id,
        agent_id,
        &old_unowned_sent_printer,
        PrintErrorAction::Ignore,
        83_918_929,
        20_043,
    )
    .await;
    let old_unowned_acknowledged_printer = additional_printer(&database, tenant_id, agent_id).await;
    let old_unowned_acknowledged = sent_native(
        &commands,
        tenant_id,
        agent_id,
        &old_unowned_acknowledged_printer,
        PrintErrorAction::Stop,
        83_918_929,
        20_044,
    )
    .await;
    commands
        .mark_acknowledged(old_unowned_acknowledged.id, tenant_id, agent_id)
        .await
        .unwrap();
    let fresh_unowned_printer = additional_printer(&database, tenant_id, agent_id).await;
    let fresh_unowned = sent_native(
        &commands,
        tenant_id,
        agent_id,
        &fresh_unowned_printer,
        PrintErrorAction::Resume,
        83_918_929,
        20_045,
    )
    .await;
    let terminal_printer = additional_printer(&database, tenant_id, agent_id).await;
    let terminal = sent_native(
        &commands,
        tenant_id,
        agent_id,
        &terminal_printer,
        PrintErrorAction::Resume,
        83_918_929,
        20_046,
    )
    .await;
    commands
        .mark_succeeded(terminal.id, tenant_id, agent_id)
        .await
        .unwrap();
    let ordinary = commands
        .enqueue_printer_operation_with_audit(
            tenant_id,
            &printer_id,
            PrinterOperationKind::Pause,
            native_audit_actor(),
        )
        .await
        .unwrap();
    commands
        .mark_sent(ordinary.id, tenant_id, agent_id)
        .await
        .unwrap();
    let old_link = commands
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            link_payload("STALE-LINK"),
            native_audit_actor(),
        )
        .await
        .unwrap();

    for command_id in [
        old_owned.id,
        old_unowned_sent.id,
        old_unowned_acknowledged.id,
        terminal.id,
        ordinary.id,
        old_link.id,
    ] {
        set_command_updated_at(&database, command_id, "2026-07-01T00:00:00Z").await;
    }
    set_command_updated_at(&database, fresh_unowned.id, "2026-07-01T00:05:00Z").await;

    let failed = commands
        .fail_stale_unowned_live_commands(
            "2026-07-01T00:06:00Z",
            std::time::Duration::from_secs(300),
            &[old_owned.id],
        )
        .await
        .unwrap();

    assert_eq!(failed, 3);
    for command_id in [old_owned.id, fresh_unowned.id, ordinary.id] {
        assert_eq!(
            load(&commands, tenant_id, command_id).await.status,
            CommandStatus::Sent
        );
    }
    assert_eq!(
        load(&commands, tenant_id, old_unowned_sent.id)
            .await
            .error
            .as_deref(),
        Some("live printer operation owner unavailable before completion")
    );
    assert_eq!(
        load(&commands, tenant_id, old_unowned_acknowledged.id)
            .await
            .error
            .as_deref(),
        Some("live printer operation owner unavailable before completion")
    );
    assert_eq!(
        load(&commands, tenant_id, terminal.id).await.status,
        CommandStatus::Succeeded
    );
    assert_eq!(
        load(&commands, tenant_id, old_link.id)
            .await
            .error
            .as_deref(),
        Some("printer link dispatch expired before completion")
    );
}

async fn setup() -> (
    crate::db::Database,
    TenantId,
    AgentId,
    String,
    CommandRepository,
    AuditEventRepository,
) {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let tenant = tenants
        .create("native-print-error", "Native Print Error")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        &database,
        tenant.id,
        agent.id,
        Some("A1"),
    )
    .await
    .unwrap();
    let audit = AuditEventRepository::new(database.clone());
    (database, tenant.id, agent.id, printer_id, commands, audit)
}

async fn sent_native(
    commands: &CommandRepository,
    tenant_id: TenantId,
    agent_id: AgentId,
    printer_id: &str,
    action: PrintErrorAction,
    print_error: u32,
    sequence_id: u64,
) -> CommandRecord {
    commands
        .create_printer_operation_sent_with_audit(
            tenant_id,
            printer_id,
            agent_id,
            native_operation(action, print_error, sequence_id),
            native_audit_actor(),
        )
        .await
        .unwrap()
}

async fn additional_printer(
    database: &crate::db::Database,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> String {
    crate::repositories::test_helpers::insert_printer_fixture_with_model(
        database,
        tenant_id,
        agent_id,
        Some("A1"),
    )
    .await
    .unwrap()
}

fn web_recovery_input(
    action: PrintErrorAction,
    expected_agent_id: AgentId,
    expected_session_id: &str,
) -> WebPrintErrorRecovery {
    WebPrintErrorRecovery {
        action,
        error_generation: 9,
        expected_agent_id,
        expected_session_id: expected_session_id.to_owned(),
    }
}

async fn seed_web_recovery_state(
    database: &crate::db::Database,
    printer_id: &str,
    session_id: &str,
) {
    let printer = crate::entities::printers::Entity::find_by_id(printer_id)
        .one(&database.sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    let mut active = printer.into_active_model();
    active.serial_number = Set("20P123456789".to_owned());
    active.status = Set("RUNNING".to_owned());
    active.print_task_generation = Set(9);
    active.print_error_generation = Set(9);
    active.print_job_attr = Set(Some(0x10));
    active.print_error_task_generation = Set(Some(9));
    active.print_error_session_id = Set(Some(session_id.to_owned()));
    active.print_error_received_at = Set(Some("2026-07-10T00:00:00Z".to_owned()));
    active.print_gcode_state = Set(Some("PAUSE".to_owned()));
    active.print_error = Set(Some(83_918_929));
    active.print_job_id = Set(Some("job-7".to_owned()));
    active.update(&database.sea_orm_connection()).await.unwrap();
}

fn native_operation(
    error_action: PrintErrorAction,
    print_error: u32,
    sequence_id: u64,
) -> PrinterOperationKind {
    PrinterOperationKind::HandlePrintError {
        error_action,
        print_error,
        printer_job_id: "job-7".to_owned(),
        sequence_id,
    }
}

fn native_audit_actor() -> AuditActor {
    AuditActor::tenant_token(None, "repository-native-print-error", vec!["*"])
}

async fn load(
    commands: &CommandRepository,
    tenant_id: TenantId,
    command_id: pandar_core::CommandId,
) -> CommandRecord {
    commands
        .get_for_tenant(tenant_id, command_id)
        .await
        .unwrap()
        .unwrap()
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TestPrintErrorAuditMetadata {
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
