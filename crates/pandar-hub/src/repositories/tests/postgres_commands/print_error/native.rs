use super::*;

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
