use pandar_core::{AgentId, CommandId, CommandStatus};
use serde_json::Value;

use super::*;
use crate::repositories::{AuditActor, PrintProjectFilePayload, PrinterOperationKind};

#[tokio::test]
async fn command_enqueue_rejects_missing_agent() {
    let (_, tenants, _, _, commands, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();

    let err = commands
        .enqueue_refresh_printers(tenant.id, AgentId::new())
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::MissingAgent));
}

#[tokio::test]
async fn command_enqueue_rejects_wrong_tenant() {
    let (_, tenants, agents, _, commands, _) = repositories().await;
    let acme = tenants.create("acme", "Acme Labs").await.unwrap();
    let beta = tenants.create("beta", "Beta Labs").await.unwrap();
    let agent = agents.create(acme.id, "agent").await.unwrap();

    let err = commands
        .enqueue_refresh_printers(beta.id, agent.id)
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::CommandOwnershipMismatch));
}

#[tokio::test]
async fn command_queue_filters_by_tenant_and_agent() {
    let (_, tenants, agents, _, commands, _) = repositories().await;
    let acme = tenants.create("acme", "Acme Labs").await.unwrap();
    let beta = tenants.create("beta", "Beta Labs").await.unwrap();
    let acme_agent = agents.create(acme.id, "agent").await.unwrap();
    let other_acme_agent = agents.create(acme.id, "other").await.unwrap();
    let beta_agent = agents.create(beta.id, "agent").await.unwrap();

    let expected = commands
        .enqueue_refresh_printers(acme.id, acme_agent.id)
        .await
        .unwrap();
    commands
        .enqueue_refresh_printers(acme.id, other_acme_agent.id)
        .await
        .unwrap();
    commands
        .enqueue_refresh_printers(beta.id, beta_agent.id)
        .await
        .unwrap();

    assert_eq!(
        commands
            .next_queued_for_agent(acme.id, acme_agent.id)
            .await
            .unwrap()
            .unwrap()
            .id,
        expected.id
    );
}

#[tokio::test]
async fn command_enqueue_print_project_file_persists_payload_and_printer() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();

    let command = commands
        .enqueue_print_project_file(
            tenant.id,
            agent.id,
            &printer_id,
            print_payload(&printer_id, "serial-explicit"),
        )
        .await
        .unwrap();
    let payload: Value = serde_json::from_str(&command.payload_json).unwrap();

    assert_eq!(command.kind, "print_project_file");
    assert_eq!(command.status, CommandStatus::Queued);
    assert_eq!(command.printer_id.as_deref(), Some(printer_id.as_str()));
    assert_eq!(payload["job_id"], "job-1");
    assert_eq!(payload["artifact_id"], "artifact-1");
    assert_eq!(payload["printer_id"], printer_id);
    assert_eq!(payload["serial_number"], "serial-explicit");
    assert_eq!(payload["filename"], "plate.3mf");
    assert_eq!(payload["storage_path"], "tenant/artifact/plate.3mf");
    assert_eq!(
        payload["artifact_download_path"],
        "/api/v1/agents/agent-1/artifacts/artifact-1"
    );
    assert_eq!(payload["size_bytes"], 3);
    assert_eq!(payload["plate_id"], 1);
    assert_eq!(payload["use_ams"], true);
    assert_eq!(payload["flow_cali"], false);
    assert_eq!(payload["timelapse"], true);
}

#[tokio::test]
async fn command_enqueue_print_project_file_rejects_missing_printer() {
    let (_, tenants, agents, _, commands, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = uuid::Uuid::new_v4().to_string();

    let err = commands
        .enqueue_print_project_file(
            tenant.id,
            agent.id,
            &printer_id,
            print_payload(&printer_id, "SERIAL1"),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::MissingPrinter));
}

#[tokio::test]
async fn command_enqueue_print_project_file_rejects_wrong_printer_owner() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let acme = tenants.create("acme", "Acme Labs").await.unwrap();
    let beta = tenants.create("beta", "Beta Labs").await.unwrap();
    let acme_agent = agents.create(acme.id, "agent").await.unwrap();
    let beta_agent = agents.create(beta.id, "agent").await.unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture(
        &database,
        beta.id,
        beta_agent.id,
    )
    .await
    .unwrap();

    let err = commands
        .enqueue_print_project_file(
            acme.id,
            acme_agent.id,
            &printer_id,
            print_payload(&printer_id, "SERIAL1"),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::MissingPrinter));
}

#[tokio::test]
async fn command_enqueue_printer_operation_derives_agent_persists_payload_and_audits() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let audit = AuditEventRepository::new(database.clone());
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        &database,
        tenant.id,
        agent.id,
        Some("A1"),
    )
    .await
    .unwrap();

    let command = commands
        .enqueue_printer_operation_with_audit(
            tenant.id,
            &printer_id,
            PrinterOperationKind::SetPrintSpeed { speed_mode: 3 },
            test_audit_actor(),
        )
        .await
        .unwrap();
    let payload: Value = serde_json::from_str(&command.payload_json).unwrap();

    assert_eq!(command.kind, "printer_operation");
    assert_eq!(command.status, CommandStatus::Queued);
    assert_eq!(command.agent_id, agent.id);
    assert_eq!(command.printer_id.as_deref(), Some(printer_id.as_str()));
    assert_eq!(payload["printer_id"], printer_id);
    assert_eq!(payload["serial_number"], format!("serial-{printer_id}"));
    assert_eq!(payload["operation"]["type"], "set_print_speed");
    assert_eq!(payload["operation"]["speed_mode"], 3);

    let events = audit.list_for_tenant(tenant.id).await.unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "printer.dispatch_control")
        .expect("printer control audit event");
    assert_eq!(event.target_type, "printer");
    assert_eq!(event.target_id.as_deref(), Some(printer_id.as_str()));
    let metadata: Value = serde_json::from_str(&event.metadata_json).unwrap();
    assert_eq!(metadata["agent_id"], agent.id.to_string());
    assert_eq!(metadata["serial_number"], format!("serial-{printer_id}"));
    assert_eq!(metadata["action"], "set_print_speed");
    assert_eq!(metadata["speed_mode"], 3);
}

#[tokio::test]
async fn command_enqueue_printer_operation_rejects_unknown_model_before_insert() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let audit = AuditEventRepository::new(database.clone());
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        &database,
        tenant.id,
        agent.id,
        Some("Mystery Model"),
    )
    .await
    .unwrap();

    let err = commands
        .enqueue_printer_operation_with_audit(
            tenant.id,
            &printer_id,
            PrinterOperationKind::Pause,
            test_audit_actor(),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::PrinterControlUnavailable));
    assert_eq!(commands.count().await.unwrap(), 0);
    assert!(audit.list_for_tenant(tenant.id).await.unwrap().is_empty());
}

#[tokio::test]
async fn command_enqueue_printer_operation_rejects_invalid_speed() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        &database,
        tenant.id,
        agent.id,
        Some("A1"),
    )
    .await
    .unwrap();

    for operation in [
        PrinterOperationKind::SetPrintSpeed { speed_mode: 0 },
        PrinterOperationKind::SetPrintSpeed { speed_mode: 5 },
        PrinterOperationKind::MoveAxes {
            movements: Vec::new(),
            feedrate_mm_per_min: None,
        },
        PrinterOperationKind::MoveAxes {
            movements: vec![crate::repositories::PrinterAxisMovement {
                axis: crate::repositories::PrinterAxis::X,
                delta_mm: 51.0,
            }],
            feedrate_mm_per_min: None,
        },
        PrinterOperationKind::MoveAxes {
            movements: vec![crate::repositories::PrinterAxisMovement {
                axis: crate::repositories::PrinterAxis::Y,
                delta_mm: 5.0,
            }],
            feedrate_mm_per_min: Some(12_001),
        },
        PrinterOperationKind::MoveAxes {
            movements: vec![
                crate::repositories::PrinterAxisMovement {
                    axis: crate::repositories::PrinterAxis::X,
                    delta_mm: 5.0,
                },
                crate::repositories::PrinterAxisMovement {
                    axis: crate::repositories::PrinterAxis::X,
                    delta_mm: 6.0,
                },
            ],
            feedrate_mm_per_min: None,
        },
        PrinterOperationKind::SetHotendTemperature {
            temperature_celsius: 301,
            wait: false,
        },
    ] {
        let err = commands
            .enqueue_printer_operation_with_audit(
                tenant.id,
                &printer_id,
                operation,
                test_audit_actor(),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, RepositoryError::InvalidPrinterControl));
    }
    assert_eq!(commands.count().await.unwrap(), 0);
}

#[tokio::test]
async fn command_update_rejects_missing_command() {
    let (_, _, commands, tenant, agent) = command_repositories().await;

    let err = commands
        .mark_sent(CommandId::new(), tenant.id, agent.id)
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::MissingCommand));
}

fn print_payload(printer_id: &str, serial_number: &str) -> PrintProjectFilePayload {
    PrintProjectFilePayload {
        job_id: "job-1".to_string(),
        artifact_id: "artifact-1".to_string(),
        printer_id: printer_id.to_string(),
        serial_number: serial_number.to_string(),
        filename: "plate.3mf".to_string(),
        storage_path: "tenant/artifact/plate.3mf".to_string(),
        artifact_download_path: "/api/v1/agents/agent-1/artifacts/artifact-1".to_string(),
        size_bytes: 3,
        plate_id: 1,
        use_ams: true,
        flow_cali: false,
        timelapse: true,
        ams_mapping_json: None,
        ams_mapping2_json: None,
    }
}

fn test_audit_actor() -> AuditActor {
    AuditActor::tenant_token(None, "repository-test-token", vec!["*"])
}

#[tokio::test]
async fn command_update_rejects_wrong_tenant() {
    let (tenants, _, commands, tenant, agent) = command_repositories().await;
    let other = tenants.create("beta", "Beta Labs").await.unwrap();
    let command = commands
        .enqueue_refresh_printers(tenant.id, agent.id)
        .await
        .unwrap();

    let err = commands
        .mark_sent(command.id, other.id, agent.id)
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::CommandOwnershipMismatch));
}

#[tokio::test]
async fn command_update_rejects_wrong_agent() {
    let (_, agents, commands, tenant, agent) = command_repositories().await;
    let other = agents.create(tenant.id, "other").await.unwrap();
    let command = commands
        .enqueue_refresh_printers(tenant.id, agent.id)
        .await
        .unwrap();

    let err = commands
        .mark_sent(command.id, tenant.id, other.id)
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::CommandOwnershipMismatch));
}

#[tokio::test]
async fn command_sent_ack_success_flow() {
    let (_, _, commands, tenant, agent) = command_repositories().await;
    let command = commands
        .enqueue_refresh_printers(tenant.id, agent.id)
        .await
        .unwrap();

    let sent = commands
        .mark_sent(command.id, tenant.id, agent.id)
        .await
        .unwrap();
    assert_eq!(sent.status, CommandStatus::Sent);
    let acked = commands
        .mark_acknowledged(command.id, tenant.id, agent.id)
        .await
        .unwrap();
    assert_eq!(acked.status, CommandStatus::Acknowledged);
    let succeeded = commands
        .mark_succeeded(command.id, tenant.id, agent.id)
        .await
        .unwrap();
    assert_eq!(succeeded.status, CommandStatus::Succeeded);
}

#[tokio::test]
async fn command_ack_failure_marks_failed() {
    let (_, _, commands, tenant, agent) = command_repositories().await;
    let command_id = enqueue_sent(&commands, tenant.id, agent.id).await;

    let failed = commands
        .mark_failed(command_id, tenant.id, agent.id, "rejected")
        .await
        .unwrap();

    assert_eq!(failed.status, CommandStatus::Failed);
    assert_eq!(failed.error.as_deref(), Some("rejected"));
}

#[tokio::test]
async fn command_result_failure_marks_failed() {
    let (_, _, commands, tenant, agent) = command_repositories().await;
    let command_id = enqueue_sent(&commands, tenant.id, agent.id).await;
    commands
        .mark_acknowledged(command_id, tenant.id, agent.id)
        .await
        .unwrap();

    let failed = commands
        .mark_failed(command_id, tenant.id, agent.id, "printer unavailable")
        .await
        .unwrap();

    assert_eq!(failed.status, CommandStatus::Failed);
    assert_eq!(failed.error.as_deref(), Some("printer unavailable"));
}
