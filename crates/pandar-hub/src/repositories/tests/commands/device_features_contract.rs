use super::*;

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
