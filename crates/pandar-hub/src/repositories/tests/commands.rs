use pandar_core::{AgentId, CommandId, CommandStatus};
use serde::Deserialize;

mod print_error;

use super::*;
use crate::repositories::{
    AuditActor, LinkPrinterPayload, PrintProjectFilePayload, PrinterOperationKind,
    PrinterOperationPayload, RefreshPrinterMaterialsPayload,
};

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
    let payload: PrintProjectFilePayload = serde_json::from_str(&command.payload_json).unwrap();

    assert_eq!(command.kind, "print_project_file");
    assert_eq!(command.status, CommandStatus::Queued);
    assert_eq!(command.printer_id.as_deref(), Some(printer_id.as_str()));
    assert_eq!(payload, print_payload(&printer_id, "serial-explicit"));
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
    let payload: PrinterOperationPayload = serde_json::from_str(&command.payload_json).unwrap();

    assert_eq!(command.kind, "printer_operation");
    assert_eq!(command.status, CommandStatus::Queued);
    assert_eq!(command.agent_id, agent.id);
    assert_eq!(command.printer_id.as_deref(), Some(printer_id.as_str()));
    assert_eq!(
        payload,
        PrinterOperationPayload {
            printer_id: printer_id.clone(),
            serial_number: format!("serial-{printer_id}"),
            operation: PrinterOperationKind::SetPrintSpeed { speed_mode: 3 },
        }
    );

    let events = audit.list_for_tenant(tenant.id).await.unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "printer.dispatch_control")
        .expect("printer control audit event");
    assert_eq!(event.target_type, "printer");
    assert_eq!(event.target_id.as_deref(), Some(printer_id.as_str()));
    let metadata: TestPrintSpeedAuditMetadata = serde_json::from_str(&event.metadata_json).unwrap();
    assert_eq!(
        metadata,
        TestPrintSpeedAuditMetadata {
            agent_id: agent.id.to_string(),
            serial_number: format!("serial-{printer_id}"),
            action: "set_print_speed".to_owned(),
            speed_mode: 3,
            audit: test_audit_metadata(),
        }
    );
}

#[tokio::test]
async fn gcode_line_operation_round_trips_exact_param_and_audit_omits_content() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
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
    assert!(operation.required_device_features().is_empty());

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

    assert_eq!(command.status, CommandStatus::Queued);
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
async fn command_enqueue_refresh_printer_materials_derives_agent_persists_payload_and_audits() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let audit = AuditEventRepository::new(database.clone());
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
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
    assert_eq!(command.status, CommandStatus::Queued);
    assert_eq!(command.agent_id, agent.id);
    assert_eq!(command.printer_id.as_deref(), Some(printer_id.as_str()));
    assert_eq!(payload.printer_id, printer_id);
    assert_eq!(
        payload.serial_number,
        format!("serial-{}", payload.printer_id)
    );

    let events = audit.list_for_tenant(tenant.id).await.unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "printer.refresh_materials")
        .expect("refresh materials audit event");
    assert_eq!(event.target_type, "printer");
    assert_eq!(
        event.target_id.as_deref(),
        Some(payload.printer_id.as_str())
    );
    let metadata: TestRefreshPrinterMaterialsAuditMetadata =
        serde_json::from_str(&event.metadata_json).unwrap();
    assert_eq!(
        metadata,
        TestRefreshPrinterMaterialsAuditMetadata {
            agent_id: agent.id.to_string(),
            printer_id: payload.printer_id,
            serial_number: payload.serial_number,
            audit: test_audit_metadata(),
        }
    );
}

#[tokio::test]
async fn command_create_link_printer_sent_persists_redacted_payload_and_audit() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let audit = AuditEventRepository::new(database.clone());
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let access_code = "SECRET-LINK-CODE";

    let command = commands
        .create_link_printer_sent_with_audit(
            tenant.id,
            agent.id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: Some("Office X1C".to_owned()),
            },
            test_audit_actor(),
        )
        .await
        .unwrap();

    assert_eq!(command.kind, "link_printer");
    assert_eq!(command.status, CommandStatus::Sent);
    assert_eq!(command.printer_id, None);
    assert!(!command.payload_json.contains(access_code));
    let payload: TestRedactedLinkPrinterPayload =
        serde_json::from_str(&command.payload_json).unwrap();
    assert_eq!(
        payload,
        TestRedactedLinkPrinterPayload {
            printer_type: "BambuLab".to_owned(),
            host: "192.0.2.10".to_owned(),
            access_code: "[redacted]".to_owned(),
            name: Some("Office X1C".to_owned()),
        }
    );

    let events = audit.list_for_tenant(tenant.id).await.unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "agent.link_printer")
        .expect("link printer audit event");
    assert_eq!(event.target_type, "agent");
    assert_eq!(
        event.target_id.as_deref(),
        Some(agent.id.to_string().as_str())
    );
    assert!(!event.metadata_json.contains(access_code));
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
}

#[tokio::test]
async fn stale_link_printer_cleanup_skips_owned_pending_commands() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
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
    let failed_command = commands
        .get_for_tenant(tenant.id, old_unowned.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed_command.status, CommandStatus::Failed);
    assert_eq!(
        failed_command.error.as_deref(),
        Some("printer link dispatch expired before completion"),
    );
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
            PrinterOperationKind::Pause {},
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
            required_device_features: Vec::new(),
        },
        PrinterOperationKind::MoveAxes {
            movements: vec![crate::repositories::PrinterAxisMovement {
                axis: crate::repositories::PrinterAxis::X,
                delta_mm: 51.0,
            }],
            feedrate_mm_per_min: None,
            required_device_features: Vec::new(),
        },
        PrinterOperationKind::MoveAxes {
            movements: vec![crate::repositories::PrinterAxisMovement {
                axis: crate::repositories::PrinterAxis::Y,
                delta_mm: 5.0,
            }],
            feedrate_mm_per_min: Some(12_001),
            required_device_features: Vec::new(),
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
            required_device_features: Vec::new(),
        },
        PrinterOperationKind::SetHotendTemperature {
            temperature_celsius: 301,
            wait: false,
            extruder_id: None,
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
async fn required_device_features_persist_and_legacy_operations_default_empty() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
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
        "type": "home",
        "axes": [],
        "required_device_features": ["bambu_mqtt_homing"]
    }))
    .unwrap();

    let command = commands
        .enqueue_printer_operation_with_audit(tenant.id, &printer_id, operation, test_audit_actor())
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&command.payload_json).unwrap();
    assert_eq!(
        payload["operation"]["required_device_features"],
        serde_json::json!(["bambu_mqtt_homing"])
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
        serde_json::json!(["bambu_mqtt_homing"])
    );

    for legacy_json in [
        serde_json::json!({"type": "home", "axes": []}),
        serde_json::json!({
            "type": "move_axes",
            "movements": [{"axis": "x", "delta_mm": 5.0}],
            "feedrate_mm_per_min": null
        }),
    ] {
        let legacy: PrinterOperationKind = serde_json::from_value(legacy_json).unwrap();
        let encoded = serde_json::to_value(legacy).unwrap();
        assert!(encoded.get("required_device_features").is_none());
    }
}

#[tokio::test]
async fn required_device_features_repository_rejects_invalid_semantics() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let tenant = tenants
        .create("invalid-required-features", "Invalid Required Features")
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
    let cases = [
        serde_json::json!({
            "type": "home",
            "axes": ["x"],
            "required_device_features": ["bambu_mqtt_homing"]
        }),
        serde_json::json!({
            "type": "home",
            "axes": [],
            "required_device_features": ["bambu_mqtt_homing", "bambu_mqtt_homing"]
        }),
        serde_json::json!({
            "type": "home",
            "axes": [],
            "required_device_features": ["bambu_mqtt_axis_control"]
        }),
        serde_json::json!({
            "type": "move_axes",
            "movements": [
                {"axis": "x", "delta_mm": 1.0},
                {"axis": "y", "delta_mm": 1.0}
            ],
            "feedrate_mm_per_min": null,
            "required_device_features": ["bambu_mqtt_axis_control"]
        }),
        serde_json::json!({
            "type": "move_axes",
            "movements": [{"axis": "x", "delta_mm": 2.0}],
            "feedrate_mm_per_min": null,
            "required_device_features": ["bambu_mqtt_axis_control"]
        }),
        serde_json::json!({
            "type": "move_axes",
            "movements": [{"axis": "x", "delta_mm": 10.0}],
            "feedrate_mm_per_min": 6_000,
            "required_device_features": ["bambu_mqtt_axis_control"]
        }),
    ];

    for value in cases {
        let operation: PrinterOperationKind = serde_json::from_value(value).unwrap();
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
    for feature in ["unspecified", "unknown"] {
        assert!(
            serde_json::from_value::<PrinterOperationKind>(serde_json::json!({
                "type": "home",
                "axes": [],
                "required_device_features": [feature]
            }))
            .is_err()
        );
    }
    assert_eq!(commands.count().await.unwrap(), 0);
}

#[tokio::test]
async fn required_device_features_cannot_bypass_queued_dispatch_through_sent_helper() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let audit = AuditEventRepository::new(database.clone());
    let tenant = tenants
        .create("required-features-sent", "Required Features Sent")
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
        "type": "home",
        "axes": [],
        "required_device_features": ["bambu_mqtt_homing"]
    }))
    .unwrap();

    let err = commands
        .create_printer_operation_sent_with_audit(
            tenant.id,
            &printer_id,
            agent.id,
            operation,
            test_audit_actor(),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::InvalidPrinterControl));
    assert_eq!(commands.count().await.unwrap(), 0);
    assert!(audit.list_for_tenant(tenant.id).await.unwrap().is_empty());
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
        bed_leveling: false,
        auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
        flow_cali: false,
        auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
        auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
        timelapse: true,
        ams_mapping_json: None,
        ams_mapping2_json: None,
        ams_mapping_info_json: None,
    }
}

async fn set_command_updated_at(
    database: &crate::db::Database,
    command_id: CommandId,
    updated_at: &str,
) {
    match database {
        crate::db::Database::Sqlite(pool) => {
            sqlx::query("UPDATE commands SET updated_at = ?2 WHERE id = ?1")
                .bind(command_id.to_string())
                .bind(updated_at)
                .execute(pool)
                .await
                .unwrap();
        }
        crate::db::Database::Postgres(pool) => {
            sqlx::query("UPDATE commands SET updated_at = $2 WHERE id = $1")
                .bind(command_id.to_string())
                .bind(updated_at)
                .execute(pool)
                .await
                .unwrap();
        }
    }
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
    AuditActor::tenant_token(None, "repository-test-token", vec!["*"])
}

fn test_audit_metadata() -> TestAuditActorMetadata {
    TestAuditActorMetadata {
        tenant_token_id: "repository-test-token".to_owned(),
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
struct TestPrintSpeedAuditMetadata {
    agent_id: String,
    serial_number: String,
    action: String,
    speed_mode: u8,
    #[serde(flatten)]
    audit: TestAuditActorMetadata,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TestRefreshPrinterMaterialsAuditMetadata {
    agent_id: String,
    printer_id: String,
    serial_number: String,
    #[serde(flatten)]
    audit: TestAuditActorMetadata,
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
