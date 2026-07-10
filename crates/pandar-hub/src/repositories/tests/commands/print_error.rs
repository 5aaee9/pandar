use pandar_core::{AgentId, CommandRecord, CommandStatus, TenantId};
use serde::Deserialize;

use super::*;
use crate::repositories::{
    AuditActor, PrintErrorAction, PrinterOperationKind, PrinterOperationPayload,
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
    let old_unowned_sent = sent_native(
        &commands,
        tenant_id,
        agent_id,
        &printer_id,
        PrintErrorAction::Ignore,
        83_918_929,
        20_043,
    )
    .await;
    let old_unowned_acknowledged = sent_native(
        &commands,
        tenant_id,
        agent_id,
        &printer_id,
        PrintErrorAction::Stop,
        83_918_929,
        20_044,
    )
    .await;
    commands
        .mark_acknowledged(old_unowned_acknowledged.id, tenant_id, agent_id)
        .await
        .unwrap();
    let fresh_unowned = sent_native(
        &commands,
        tenant_id,
        agent_id,
        &printer_id,
        PrintErrorAction::Resume,
        83_918_929,
        20_045,
    )
    .await;
    let terminal = sent_native(
        &commands,
        tenant_id,
        agent_id,
        &printer_id,
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
