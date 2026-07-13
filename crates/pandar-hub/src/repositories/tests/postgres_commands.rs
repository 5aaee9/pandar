use pandar_core::{AgentId, AgentStatus, CommandId, CommandStatus};
use serde::Deserialize;

mod print_error;

use super::*;
use crate::repositories::tests::postgres::postgres_database;
use crate::repositories::{
    AuditActor, LinkPrinterPayload, PrinterOperationKind, PrinterOperationPayload,
    RefreshPrinterMaterialsPayload,
};

#[tokio::test]
async fn postgres_command_repository_behavior_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database);
    let acme = tenants.create("acme", "Acme Labs").await.unwrap();
    let beta = tenants.create("beta", "Beta Labs").await.unwrap();
    let agent = agents.create(acme.id, "agent").await.unwrap();
    let other_agent = agents.create(acme.id, "other").await.unwrap();
    let beta_agent = agents.create(beta.id, "agent").await.unwrap();

    assert_eq!(agents.get(agent.id).await.unwrap(), Some(agent.clone()));
    assert_eq!(
        agents
            .update_connection(
                agent.id,
                AgentStatus::Online,
                Some("0.2.0"),
                "2026-06-20T01:00:00Z"
            )
            .await
            .unwrap()
            .status,
        AgentStatus::Online
    );
    assert_eq!(
        agents
            .mark_offline(agent.id, "2026-06-20T01:01:00Z")
            .await
            .unwrap()
            .status,
        AgentStatus::Offline
    );

    assert!(matches!(
        commands
            .enqueue_refresh_printers(acme.id, AgentId::new())
            .await
            .unwrap_err(),
        RepositoryError::MissingAgent
    ));
    assert!(matches!(
        commands
            .enqueue_refresh_printers(beta.id, agent.id)
            .await
            .unwrap_err(),
        RepositoryError::CommandOwnershipMismatch
    ));

    let command = commands
        .enqueue_refresh_printers(acme.id, agent.id)
        .await
        .unwrap();
    commands
        .enqueue_refresh_printers(acme.id, other_agent.id)
        .await
        .unwrap();
    commands
        .enqueue_refresh_printers(beta.id, beta_agent.id)
        .await
        .unwrap();
    assert_eq!(
        commands
            .next_queued_for_agent(acme.id, agent.id)
            .await
            .unwrap()
            .unwrap()
            .id,
        command.id
    );
    assert!(matches!(
        commands
            .mark_sent(CommandId::new(), acme.id, agent.id)
            .await
            .unwrap_err(),
        RepositoryError::MissingCommand
    ));
    assert!(matches!(
        commands
            .mark_sent(command.id, beta.id, agent.id)
            .await
            .unwrap_err(),
        RepositoryError::CommandOwnershipMismatch
    ));
    assert!(matches!(
        commands
            .mark_sent(command.id, acme.id, other_agent.id)
            .await
            .unwrap_err(),
        RepositoryError::CommandOwnershipMismatch
    ));

    assert_eq!(
        commands
            .mark_sent(command.id, acme.id, agent.id)
            .await
            .unwrap()
            .status,
        CommandStatus::Sent
    );
    assert_eq!(
        commands
            .mark_acknowledged(command.id, acme.id, agent.id)
            .await
            .unwrap()
            .status,
        CommandStatus::Acknowledged
    );
    assert_eq!(
        commands
            .mark_succeeded(command.id, acme.id, agent.id)
            .await
            .unwrap()
            .status,
        CommandStatus::Succeeded
    );
    assert_eq!(
        commands
            .mark_succeeded(command.id, acme.id, agent.id)
            .await
            .unwrap()
            .status,
        CommandStatus::Succeeded
    );

    let failed = enqueue_sent(&commands, acme.id, agent.id).await;
    let first_failure = commands
        .mark_failed(failed, acme.id, agent.id, "first")
        .await
        .unwrap();
    assert_eq!(
        commands
            .mark_failed(failed, acme.id, agent.id, "second")
            .await
            .unwrap()
            .error,
        first_failure.error
    );
    assert!(matches!(
        commands
            .mark_acknowledged(failed, acme.id, agent.id)
            .await
            .unwrap_err(),
        RepositoryError::InvalidCommandTransition { .. }
    ));

    let ack_failed = enqueue_sent(&commands, acme.id, agent.id).await;
    commands
        .mark_acknowledged(ack_failed, acme.id, agent.id)
        .await
        .unwrap();
    let result_failure = commands
        .mark_failed(ack_failed, acme.id, agent.id, "printer unavailable")
        .await
        .unwrap();
    assert_eq!(result_failure.status, CommandStatus::Failed);
    assert_eq!(result_failure.error.as_deref(), Some("printer unavailable"));

    let diagnostic_id = enqueue_sent(&commands, acme.id, agent.id).await;
    commands
        .mark_acknowledged(diagnostic_id, acme.id, agent.id)
        .await
        .unwrap();
    let diagnostic_result = r#"{"type":"printer_diagnostic","overall":"problem"}"#;
    let diagnostic_success = commands
        .mark_succeeded_with_result(
            diagnostic_id,
            acme.id,
            agent.id,
            Some(diagnostic_result.to_owned()),
        )
        .await
        .unwrap();
    assert_eq!(diagnostic_success.status, CommandStatus::Succeeded);
    assert_eq!(
        diagnostic_success.result_json.as_deref(),
        Some(diagnostic_result)
    );

    let unexpected_id = enqueue_sent(&commands, acme.id, agent.id).await;
    let unexpected_result = r#"{"type":"printer_diagnostic","checks":[]}"#;
    let unexpected_failure = commands
        .mark_failed_with_result(
            unexpected_id,
            acme.id,
            agent.id,
            "unexpected diagnostics failure",
            Some(unexpected_result.to_owned()),
        )
        .await
        .unwrap();
    assert_eq!(unexpected_failure.status, CommandStatus::Failed);
    assert_eq!(
        unexpected_failure.result_json.as_deref(),
        Some(unexpected_result)
    );

    assert_eq!(
        commands
            .get_for_tenant(acme.id, diagnostic_success.id)
            .await
            .unwrap()
            .unwrap()
            .result_json
            .as_deref(),
        Some(diagnostic_result)
    );
    assert_eq!(
        commands
            .get_for_tenant(beta.id, diagnostic_success.id)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn postgres_printer_operation_enqueue_behavior_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let audit = AuditEventRepository::new(database.clone());
    let tenant = tenants.create("control", "Control Labs").await.unwrap();
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
            PrinterOperationKind::Pause {},
            test_audit_actor(),
        )
        .await
        .unwrap();
    let payload: PrinterOperationPayload = serde_json::from_str(&command.payload_json).unwrap();
    assert_eq!(command.kind, "printer_operation");
    assert_eq!(command.agent_id, agent.id);
    assert_eq!(command.printer_id.as_deref(), Some(printer_id.as_str()));
    assert_eq!(
        payload,
        PrinterOperationPayload {
            printer_id: printer_id.clone(),
            serial_number: format!("serial-{printer_id}"),
            operation: PrinterOperationKind::Pause {},
        }
    );
    assert!(
        audit
            .list_for_tenant(tenant.id)
            .await
            .unwrap()
            .iter()
            .any(|event| event.action == "printer.dispatch_control")
    );

    let unsupported_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
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
            &unsupported_id,
            PrinterOperationKind::Pause {},
            test_audit_actor(),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::PrinterControlUnavailable));
}

#[tokio::test]
async fn postgres_gcode_line_round_trips_exact_param_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let audit = AuditEventRepository::new(database.clone());
    let tenant = tenants.create("gcode-line", "G-code Line").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        &database,
        tenant.id,
        agent.id,
        Some("A1"),
    )
    .await
    .unwrap();
    let param = "M620 C1 \r\n; keep  \n";
    let operation = PrinterOperationKind::GcodeLine {
        param: param.to_owned(),
    };

    let command = commands
        .enqueue_printer_operation_with_audit(
            tenant.id,
            &printer_id,
            operation.clone(),
            test_audit_actor(),
        )
        .await
        .unwrap();
    let payload: PrinterOperationPayload = serde_json::from_str(&command.payload_json).unwrap();

    assert_eq!(payload.operation, operation);
    let event = audit
        .list_for_tenant(tenant.id)
        .await
        .unwrap()
        .into_iter()
        .find(|event| event.action == "printer.dispatch_control")
        .expect("printer control audit event");
    let metadata: serde_json::Value = serde_json::from_str(&event.metadata_json).unwrap();
    assert_eq!(metadata["action"], "gcode_line");
    assert!(metadata.get("param").is_none());
    assert!(!event.metadata_json.contains("M620 C1"));
}

#[tokio::test]
async fn postgres_required_device_features_match_sqlite_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let audit = AuditEventRepository::new(database.clone());
    let tenant = tenants
        .create("required-features", "Required Features")
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
    let operation: PrinterOperationKind = serde_json::from_value(serde_json::json!({
        "type": "move_axes",
        "movements": [{"axis": "x", "delta_mm": -10.0}],
        "feedrate_mm_per_min": null,
        "required_device_features": ["bambu_mqtt_axis_control"]
    }))
    .unwrap();

    let command = commands
        .enqueue_printer_operation_with_audit(tenant.id, &printer_id, operation, test_audit_actor())
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&command.payload_json).unwrap();
    assert_eq!(
        payload["operation"]["required_device_features"],
        serde_json::json!(["bambu_mqtt_axis_control"])
    );
    let event = audit
        .list_for_tenant(tenant.id)
        .await
        .unwrap()
        .into_iter()
        .find(|event| event.action == "printer.dispatch_control")
        .unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&event.metadata_json).unwrap();
    assert_eq!(
        metadata["required_device_features"],
        serde_json::json!(["bambu_mqtt_axis_control"])
    );
}

#[tokio::test]
async fn postgres_refresh_printer_materials_command_matches_sqlite_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let tenant = tenants
        .create("refresh-materials", "Refresh Materials Labs")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();

    let command = commands
        .enqueue_refresh_printer_materials_with_audit(tenant.id, &printer_id, test_audit_actor())
        .await
        .unwrap();
    let payload: RefreshPrinterMaterialsPayload =
        serde_json::from_str(&command.payload_json).unwrap();

    assert_eq!(command.kind, "refresh_printer_materials");
    assert_eq!(command.agent_id, agent.id);
    assert_eq!(command.printer_id.as_deref(), Some(printer_id.as_str()));
    assert_eq!(payload.printer_id, printer_id);
    assert_eq!(
        payload.serial_number,
        format!("serial-{}", payload.printer_id)
    );
}

#[tokio::test]
async fn postgres_link_printer_command_behavior_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let audit = AuditEventRepository::new(database.clone());
    let tenant = tenants
        .create("link-printer", "Link Printer Labs")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let old_owned = commands
        .create_link_printer_sent_with_audit(
            tenant.id,
            agent.id,
            link_payload("OWNED"),
            test_audit_actor(),
        )
        .await
        .unwrap();
    let old_unowned = commands
        .create_link_printer_sent_with_audit(
            tenant.id,
            agent.id,
            link_payload("UNOWNED"),
            test_audit_actor(),
        )
        .await
        .unwrap();

    let payload: TestRedactedLinkPrinterPayload =
        serde_json::from_str(&old_owned.payload_json).unwrap();
    assert_eq!(old_owned.kind, "link_printer");
    assert_eq!(old_owned.status, CommandStatus::Sent);
    assert_eq!(
        payload,
        TestRedactedLinkPrinterPayload {
            printer_type: "BambuLab".to_owned(),
            host: "192.0.2.10".to_owned(),
            access_code: "[redacted]".to_owned(),
            name: Some("Office X1C".to_owned()),
        }
    );
    assert!(!old_owned.payload_json.contains("SECRET-OWNED"));
    let event = audit
        .list_for_tenant(tenant.id)
        .await
        .unwrap()
        .into_iter()
        .find(|event| event.action == "agent.link_printer")
        .expect("link printer audit event");
    let metadata: TestLinkPrinterAuditMetadata =
        serde_json::from_str(&event.metadata_json).unwrap();
    assert_eq!(
        metadata,
        TestLinkPrinterAuditMetadata {
            printer_type: "BambuLab".to_owned(),
            host: "192.0.2.10".to_owned(),
            name: Some("Office X1C".to_owned()),
            audit: test_audit_metadata(),
        }
    );
    assert!(!event.metadata_json.contains("SECRET-OWNED"));

    set_command_updated_at(&database, old_owned.id, "2026-07-01T00:00:00Z").await;
    set_command_updated_at(&database, old_unowned.id, "2026-07-01T00:00:00Z").await;

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

    assert_eq!(failed, 1);
    assert_eq!(
        commands
            .get_for_tenant(tenant.id, old_owned.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        CommandStatus::Sent,
    );
    assert_eq!(
        commands
            .get_for_tenant(tenant.id, old_unowned.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        CommandStatus::Failed,
    );
}

async fn set_command_updated_at(
    database: &crate::db::Database,
    command_id: CommandId,
    updated_at: &str,
) {
    let crate::db::Database::Postgres(pool) = database else {
        panic!("expected PostgreSQL database");
    };
    sqlx::query("UPDATE commands SET updated_at = $2 WHERE id = $1")
        .bind(command_id.to_string())
        .bind(updated_at)
        .execute(pool)
        .await
        .unwrap();
}

fn link_payload(serial: &str) -> LinkPrinterPayload {
    LinkPrinterPayload {
        printer_type: "BambuLab".to_owned(),
        host: "192.0.2.10".to_owned(),
        access_code: format!("SECRET-{serial}"),
        name: Some("Office X1C".to_owned()),
    }
}

fn test_audit_actor() -> AuditActor {
    AuditActor::tenant_token(None, "postgres-repository-test-token", vec!["*"])
}

fn test_audit_metadata() -> TestAuditActorMetadata {
    TestAuditActorMetadata {
        tenant_token_id: "postgres-repository-test-token".to_owned(),
        tenant_token_scopes: vec!["*".to_owned()],
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TestAuditActorMetadata {
    tenant_token_id: String,
    tenant_token_scopes: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TestRedactedLinkPrinterPayload {
    printer_type: String,
    host: String,
    access_code: String,
    name: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TestLinkPrinterAuditMetadata {
    printer_type: String,
    host: String,
    name: Option<String>,
    #[serde(flatten)]
    audit: TestAuditActorMetadata,
}
