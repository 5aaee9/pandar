use pandar_core::CommandStatus;
use serde::Deserialize;

use super::*;
use crate::repositories::{
    AuditActor, PrintErrorAction, PrinterOperationKind, PrinterOperationPayload,
    PrinterSnapshotUpsert, printer_operation_ownership_pause,
};

#[tokio::test]
async fn postgres_handle_print_error_rejects_same_serial_reassignment_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let audit = AuditEventRepository::new(database.clone());
    let tenant = tenants
        .create("postgres-native-owner-race", "Postgres Native Owner Race")
        .await
        .unwrap();
    let original_agent = agents.create(tenant.id, "original").await.unwrap();
    let replacement_agent = agents.create(tenant.id, "replacement").await.unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        &database,
        tenant.id,
        original_agent.id,
        Some("A1"),
    )
    .await
    .unwrap();
    let pause = printer_operation_ownership_pause::install(&printer_id);
    let create = tokio::spawn({
        let commands = commands.clone();
        let printer_id = printer_id.clone();
        async move {
            commands
                .create_printer_operation_sent_with_audit(
                    tenant.id,
                    &printer_id,
                    original_agent.id,
                    native_operation(PrintErrorAction::Resume, 83_918_929, 20_047),
                    native_audit_actor(),
                )
                .await
        }
    });
    let resume = pause.wait_until_reached().await.unwrap();
    printers
        .upsert_snapshot(
            tenant.id,
            replacement_agent.id,
            reassigned_snapshot(format!("serial-{printer_id}")),
        )
        .await
        .unwrap();
    resume.send(()).unwrap();

    assert!(matches!(
        create.await.unwrap().unwrap_err(),
        RepositoryError::PrinterControlUnavailable
    ));
    assert_eq!(commands.count().await.unwrap(), 0);
    assert!(audit.list_for_tenant(tenant.id).await.unwrap().is_empty());
}

#[tokio::test]
async fn postgres_handle_print_error_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let audit = AuditEventRepository::new(database.clone());
    let tenant = tenants
        .create("postgres-native-error", "Postgres Native Error")
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
    let serial_number = format!("serial-{printer_id}");
    let mut sent = Vec::new();

    for (action, action_name, sequence_id) in [
        (PrintErrorAction::Resume, "resume", 20_042),
        (PrintErrorAction::Ignore, "ignore", 20_043),
        (PrintErrorAction::Stop, "stop", 20_044),
    ] {
        let operation = native_operation(action, 83_918_929, sequence_id);
        let command = commands
            .create_printer_operation_sent_with_audit(
                tenant.id,
                &printer_id,
                agent.id,
                operation.clone(),
                native_audit_actor(),
            )
            .await
            .unwrap();
        assert_eq!(command.status, CommandStatus::Sent);
        assert_eq!(command.agent_id, agent.id);
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
        assert_eq!(
            serde_json::from_str::<PrinterOperationPayload>(&command.payload_json)
                .unwrap()
                .operation,
            operation
        );
        sent.push(command);
    }

    assert!(
        commands
            .next_queued_for_agent(tenant.id, agent.id)
            .await
            .unwrap()
            .is_none()
    );
    let events = audit.list_for_tenant(tenant.id).await.unwrap();
    assert_eq!(events.len(), 3);
    for (action, sequence_id) in [
        (PrintErrorAction::Resume, 20_042),
        (PrintErrorAction::Ignore, 20_043),
        (PrintErrorAction::Stop, 20_044),
    ] {
        assert!(events.iter().any(|event| {
            serde_json::from_str::<TestPrintErrorAuditMetadata>(&event.metadata_json).is_ok_and(
                |metadata| {
                    metadata
                        == TestPrintErrorAuditMetadata {
                            agent_id: agent.id.to_string(),
                            serial_number: serial_number.clone(),
                            action: "handle_print_error".to_owned(),
                            error_action: action,
                            print_error: 83_918_929,
                            printer_job_id: "job-7".to_owned(),
                            sequence_id,
                            tenant_token_id: "postgres-native-print-error".to_owned(),
                            tenant_token_scopes: vec!["*".to_owned()],
                        }
                },
            )
        }));
    }

    let queued_error = commands
        .enqueue_printer_operation_with_audit(
            tenant.id,
            &printer_id,
            native_operation(PrintErrorAction::Resume, 83_918_929, 20_045),
            native_audit_actor(),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        queued_error,
        RepositoryError::InvalidPrinterControl
    ));
    for print_error in [0, i32::MAX as u32 + 1] {
        let err = commands
            .create_printer_operation_sent_with_audit(
                tenant.id,
                &printer_id,
                agent.id,
                native_operation(PrintErrorAction::Resume, print_error, 20_046),
                native_audit_actor(),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, RepositoryError::InvalidPrinterControl));
    }
    assert_eq!(commands.count().await.unwrap(), 3);

    commands
        .mark_acknowledged(sent[1].id, tenant.id, agent.id)
        .await
        .unwrap();
    let ordinary = commands
        .enqueue_printer_operation_with_audit(
            tenant.id,
            &printer_id,
            PrinterOperationKind::Pause,
            native_audit_actor(),
        )
        .await
        .unwrap();
    commands
        .mark_sent(ordinary.id, tenant.id, agent.id)
        .await
        .unwrap();
    let old_link = commands
        .create_link_printer_sent_with_audit(
            tenant.id,
            agent.id,
            link_payload("POSTGRES-STALE-LINK"),
            native_audit_actor(),
        )
        .await
        .unwrap();
    for command_id in [sent[0].id, sent[1].id, ordinary.id, old_link.id] {
        set_command_updated_at(&database, command_id, "2026-07-01T00:00:00Z").await;
    }
    set_command_updated_at(&database, sent[2].id, "2026-07-01T00:05:00Z").await;

    let failed = commands
        .fail_stale_unowned_live_commands(
            "2026-07-01T00:06:00Z",
            std::time::Duration::from_secs(300),
            &[sent[0].id],
        )
        .await
        .unwrap();

    assert_eq!(failed, 2);
    assert_eq!(
        load(&commands, tenant.id, sent[0].id).await.status,
        CommandStatus::Sent
    );
    assert_eq!(
        load(&commands, tenant.id, sent[2].id).await.status,
        CommandStatus::Sent
    );
    assert_eq!(
        load(&commands, tenant.id, ordinary.id).await.status,
        CommandStatus::Sent
    );
    assert_eq!(
        load(&commands, tenant.id, sent[1].id)
            .await
            .error
            .as_deref(),
        Some("live printer operation owner unavailable before completion")
    );
    assert_eq!(
        load(&commands, tenant.id, old_link.id)
            .await
            .error
            .as_deref(),
        Some("printer link dispatch expired before completion")
    );
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
    AuditActor::tenant_token(None, "postgres-native-print-error", vec!["*"])
}

fn reassigned_snapshot(serial_number: String) -> PrinterSnapshotUpsert {
    PrinterSnapshotUpsert {
        serial_number,
        host: Some("192.0.2.20".to_owned()),
        access_code: None,
        name: "Reassigned Printer".to_owned(),
        model: Some("A1".to_owned()),
        status: "IDLE".to_owned(),
        observed_at: "2026-07-10T00:00:00Z".to_owned(),
        nozzle_temperatures: Vec::new(),
        active_nozzle: None,
        bed_temperature_celsius: None,
        bed_target_temperature_celsius: None,
        chamber_temperature_celsius: None,
        chamber_light_on: None,
    }
}

async fn load(
    commands: &CommandRepository,
    tenant_id: pandar_core::TenantId,
    command_id: pandar_core::CommandId,
) -> pandar_core::CommandRecord {
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
