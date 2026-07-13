use std::sync::Arc;

use pandar_core::CommandStatus;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, IntoActiveModel};
use serde::Deserialize;
use tokio::sync::Barrier;

use super::*;
use crate::repositories::{
    AuditActor, PrintErrorAction, PrinterOperationKind, PrinterOperationPayload,
    PrinterSnapshotUpsert, WebPrintErrorRecovery, printer_operation_ownership_pause,
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
        commands
            .mark_succeeded(command.id, tenant.id, agent.id)
            .await
            .unwrap();
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

    let old_owned_printer = additional_printer(&database, tenant.id, agent.id).await;
    let old_owned = commands
        .create_printer_operation_sent_with_audit(
            tenant.id,
            &old_owned_printer,
            agent.id,
            native_operation(PrintErrorAction::Resume, 83_918_929, 20_050),
            native_audit_actor(),
        )
        .await
        .unwrap();
    let acknowledged_printer = additional_printer(&database, tenant.id, agent.id).await;
    let acknowledged = commands
        .create_printer_operation_sent_with_audit(
            tenant.id,
            &acknowledged_printer,
            agent.id,
            native_operation(PrintErrorAction::Ignore, 83_918_929, 20_051),
            native_audit_actor(),
        )
        .await
        .unwrap();
    commands
        .mark_acknowledged(acknowledged.id, tenant.id, agent.id)
        .await
        .unwrap();
    let fresh_printer = additional_printer(&database, tenant.id, agent.id).await;
    let fresh = commands
        .create_printer_operation_sent_with_audit(
            tenant.id,
            &fresh_printer,
            agent.id,
            native_operation(PrintErrorAction::Stop, 83_918_929, 20_052),
            native_audit_actor(),
        )
        .await
        .unwrap();
    let ordinary = commands
        .enqueue_printer_operation_with_audit(
            tenant.id,
            &printer_id,
            PrinterOperationKind::Pause {},
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
    for command_id in [old_owned.id, acknowledged.id, ordinary.id, old_link.id] {
        set_command_updated_at(&database, command_id, "2026-07-01T00:00:00Z").await;
    }
    set_command_updated_at(&database, fresh.id, "2026-07-01T00:05:00Z").await;

    let failed = commands
        .fail_stale_unowned_live_commands(
            "2026-07-01T00:06:00Z",
            std::time::Duration::from_secs(300),
            std::time::Duration::from_secs(45),
            uuid::Uuid::new_v4(),
            &[old_owned.id],
        )
        .await
        .unwrap();

    assert_eq!(failed, 2);
    assert_eq!(
        load(&commands, tenant.id, old_owned.id).await.status,
        CommandStatus::Sent
    );
    assert_eq!(
        load(&commands, tenant.id, fresh.id).await.status,
        CommandStatus::Sent
    );
    assert_eq!(
        load(&commands, tenant.id, ordinary.id).await.status,
        CommandStatus::Sent
    );
    assert_eq!(
        load(&commands, tenant.id, acknowledged.id)
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

#[tokio::test]
async fn postgres_web_and_plugin_recovery_share_single_flight_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let tenant = tenants
        .create("postgres-web-recovery", "Postgres Web Recovery")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let session_id = "postgres-web-recovery-session";
    agents
        .claim_online_session(
            tenant.id,
            agent.id,
            session_id,
            "test",
            "2026-07-10T00:00:00Z",
        )
        .await
        .unwrap();
    let printer_id = additional_printer(&database, tenant.id, agent.id).await;
    seed_web_recovery_state(&database, &printer_id, session_id, "20P123456789").await;

    let (left, right) = tokio::join!(
        commands.create_web_print_error_sent_with_audit(
            tenant.id,
            &printer_id,
            web_recovery_input(PrintErrorAction::Resume, agent.id, session_id),
            native_audit_actor(),
        ),
        commands.create_web_print_error_sent_with_audit(
            tenant.id,
            &printer_id,
            web_recovery_input(PrintErrorAction::Ignore, agent.id, session_id),
            native_audit_actor(),
        ),
    );
    let first = match (left, right) {
        (Ok(first), Err(RepositoryError::PrinterControlUnavailable))
        | (Err(RepositoryError::PrinterControlUnavailable), Ok(first)) => first,
        (left, right) => panic!("expected one PostgreSQL recovery winner: {left:?}, {right:?}"),
    };
    assert_eq!(commands.count().await.unwrap(), 1);
    commands
        .mark_succeeded(first.command.id, tenant.id, agent.id)
        .await
        .unwrap();

    let plugin = commands
        .create_printer_operation_sent_with_audit(
            tenant.id,
            &printer_id,
            agent.id,
            native_operation(PrintErrorAction::Stop, 83_918_929, 20_060),
            native_audit_actor(),
        )
        .await
        .unwrap();
    let error = commands
        .create_web_print_error_sent_with_audit(
            tenant.id,
            &printer_id,
            web_recovery_input(PrintErrorAction::Stop, agent.id, session_id),
            native_audit_actor(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, RepositoryError::PrinterControlUnavailable));
    commands
        .mark_failed(plugin.id, tenant.id, agent.id, "terminal")
        .await
        .unwrap();
    let retry = commands
        .create_web_print_error_sent_with_audit(
            tenant.id,
            &printer_id,
            web_recovery_input(PrintErrorAction::Stop, agent.id, session_id),
            native_audit_actor(),
        )
        .await
        .unwrap();
    assert_eq!(retry.command.status, CommandStatus::Sent);
    commands
        .mark_succeeded(retry.command.id, tenant.id, agent.id)
        .await
        .unwrap();

    let web_first_printer = additional_printer(&database, tenant.id, agent.id).await;
    seed_web_recovery_state(&database, &web_first_printer, session_id, "20PWEB123456").await;
    let before_web_first = commands.count().await.unwrap();
    let web_first = commands
        .create_web_print_error_sent_with_audit(
            tenant.id,
            &web_first_printer,
            web_recovery_input(PrintErrorAction::Resume, agent.id, session_id),
            native_audit_actor(),
        )
        .await
        .unwrap();
    let error = commands
        .create_printer_operation_sent_with_audit(
            tenant.id,
            &web_first_printer,
            agent.id,
            native_operation(PrintErrorAction::Ignore, 83_918_929, 20_061),
            native_audit_actor(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, RepositoryError::PrinterControlUnavailable));
    assert_eq!(commands.count().await.unwrap(), before_web_first + 1);
    commands
        .mark_succeeded(web_first.command.id, tenant.id, agent.id)
        .await
        .unwrap();

    let mixed_printer = additional_printer(&database, tenant.id, agent.id).await;
    seed_web_recovery_state(&database, &mixed_printer, session_id, "20PMIX123456").await;
    let before_mixed = commands.count().await.unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let web = tokio::spawn({
        let barrier = barrier.clone();
        let commands = commands.clone();
        let printer_id = mixed_printer.clone();
        async move {
            barrier.wait().await;
            commands
                .create_web_print_error_sent_with_audit(
                    tenant.id,
                    &printer_id,
                    web_recovery_input(PrintErrorAction::Resume, agent.id, session_id),
                    native_audit_actor(),
                )
                .await
        }
    });
    let plugin = tokio::spawn({
        let barrier = barrier.clone();
        let commands = commands.clone();
        let printer_id = mixed_printer.clone();
        async move {
            barrier.wait().await;
            commands
                .create_printer_operation_sent_with_audit(
                    tenant.id,
                    &printer_id,
                    agent.id,
                    native_operation(PrintErrorAction::Stop, 83_918_929, 20_062),
                    native_audit_actor(),
                )
                .await
        }
    });
    barrier.wait().await;
    let web = web.await.unwrap();
    let plugin = plugin.await.unwrap();
    let winner_id = match (web, plugin) {
        (Ok(web), Err(RepositoryError::PrinterControlUnavailable)) => web.command.id,
        (Err(RepositoryError::PrinterControlUnavailable), Ok(plugin)) => plugin.id,
        (web, plugin) => {
            panic!("expected one mixed PostgreSQL recovery winner: {web:?}, {plugin:?}")
        }
    };
    assert_eq!(commands.count().await.unwrap(), before_mixed + 1);
    commands
        .mark_succeeded(winner_id, tenant.id, agent.id)
        .await
        .unwrap();
}

#[tokio::test]
async fn postgres_web_recovery_revalidates_ownership_and_session_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let tenant = tenants
        .create("postgres-web-revalidation", "Postgres Web Revalidation")
        .await
        .unwrap();
    let original = agents.create(tenant.id, "original").await.unwrap();
    let replacement = agents.create(tenant.id, "replacement").await.unwrap();
    let session_id = "postgres-web-revalidation-session";
    agents
        .claim_online_session(
            tenant.id,
            original.id,
            session_id,
            "test",
            "2026-07-10T00:00:00Z",
        )
        .await
        .unwrap();

    let ownership_printer = additional_printer(&database, tenant.id, original.id).await;
    let ownership_serial = "20POWN123456";
    seed_web_recovery_state(&database, &ownership_printer, session_id, ownership_serial).await;
    let pause = printer_operation_ownership_pause::install(&ownership_printer);
    let recovery = tokio::spawn({
        let commands = commands.clone();
        let printer_id = ownership_printer.clone();
        async move {
            commands
                .create_web_print_error_sent_with_audit(
                    tenant.id,
                    &printer_id,
                    web_recovery_input(PrintErrorAction::Resume, original.id, session_id),
                    native_audit_actor(),
                )
                .await
        }
    });
    let resume = pause.wait_until_reached().await.unwrap();
    printers
        .upsert_snapshot(
            tenant.id,
            replacement.id,
            reassigned_snapshot(ownership_serial.to_owned()),
        )
        .await
        .unwrap();
    resume.send(()).unwrap();
    assert!(matches!(
        recovery.await.unwrap().unwrap_err(),
        RepositoryError::PrinterControlUnavailable
    ));
    assert_eq!(commands.count().await.unwrap(), 0);

    let session_printer = additional_printer(&database, tenant.id, original.id).await;
    seed_web_recovery_state(&database, &session_printer, session_id, "20PSES123456").await;
    let pause = printer_operation_ownership_pause::install(&session_printer);
    let recovery = tokio::spawn({
        let commands = commands.clone();
        let printer_id = session_printer.clone();
        async move {
            commands
                .create_web_print_error_sent_with_audit(
                    tenant.id,
                    &printer_id,
                    web_recovery_input(PrintErrorAction::Stop, original.id, session_id),
                    native_audit_actor(),
                )
                .await
        }
    });
    let resume = pause.wait_until_reached().await.unwrap();
    agents
        .claim_online_session(
            tenant.id,
            original.id,
            "postgres-web-replacement-session",
            "replacement",
            "2026-07-10T00:00:01Z",
        )
        .await
        .unwrap();
    resume.send(()).unwrap();
    assert!(matches!(
        recovery.await.unwrap().unwrap_err(),
        RepositoryError::PrinterControlUnavailable
    ));
    assert_eq!(commands.count().await.unwrap(), 0);
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

async fn additional_printer(
    database: &crate::db::Database,
    tenant_id: pandar_core::TenantId,
    agent_id: pandar_core::AgentId,
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
    expected_agent_id: pandar_core::AgentId,
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
    serial_number: &str,
) {
    let printer = crate::entities::printers::Entity::find_by_id(printer_id)
        .one(&database.sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    let mut active = printer.into_active_model();
    active.serial_number = Set(serial_number.to_owned());
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
