use super::*;

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
