use super::*;

#[tokio::test]
async fn refresh_printer_materials_command_emits_material_snapshot_and_success() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakeGateway::ok_with_materials([refresh_result(
        snapshot("SERIAL123", "garage", Some("A1 Mini"), "READY"),
        material_result("SERIAL123", Some("printer-1")),
    )]);
    let (sender, mut events) = mpsc::channel(8);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        refresh_materials_command(command_id.clone(), "printer-1", "SERIAL123"),
    )
    .await
    .unwrap();

    assert!(matches!(
        events.recv().await.unwrap().event,
        Some(agent_event::Event::CommandAck(_))
    ));
    assert!(matches!(
        events.recv().await.unwrap().event,
        Some(agent_event::Event::PrinterMaterialsSnapshot(_))
    ));
    assert!(
        matches!(events.recv().await.unwrap().event, Some(agent_event::Event::CommandResult(result)) if result.success)
    );
}

#[tokio::test]
async fn refresh_printers_emits_snapshot_then_material_snapshot_then_success() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakeGateway::ok_with_materials([refresh_result(
        snapshot("SERIAL1", "garage", Some("A1 Mini"), "READY"),
        material_result("SERIAL1", None),
    )]);
    let (sender, mut receiver) = mpsc::channel(4);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        refresh_command(command_id.clone()),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_snapshot(
        receiver.recv().await.unwrap(),
        "SERIAL1",
        "garage",
        "A1 Mini",
        "READY",
    );
    assert_material_snapshot(receiver.recv().await.unwrap(), "SERIAL1", None);
    assert_eq!(
        receiver.recv().await.unwrap(),
        success_event(&config, &command_id)
    );
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn refresh_printers_succeeds_with_snapshot_only_when_materials_absent() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakeGateway::ok_with_materials([PrinterRefreshResult {
        snapshot: snapshot("SERIAL1", "garage", Some("A1 Mini"), "READY"),
        materials: None,
    }]);
    let (sender, mut receiver) = mpsc::channel(3);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        refresh_command(command_id.clone()),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_snapshot(
        receiver.recv().await.unwrap(),
        "SERIAL1",
        "garage",
        "A1 Mini",
        "READY",
    );
    assert_eq!(
        receiver.recv().await.unwrap(),
        success_event(&config, &command_id)
    );
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn refresh_printer_materials_missing_serial_and_timeout_fail_with_redacted_errors() {
    let config = test_config();
    let gateway = FakeGateway::material_fail_with_access_code(
        "ACCESS-CODE-SECRET",
        anyhow::anyhow!("no configured Bambu printer matches serial SERIAL404"),
    );
    let (sender, mut receiver) = mpsc::channel(2);
    let command_id = uuid::Uuid::new_v4().to_string();

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        refresh_materials_command(command_id.clone(), "printer-1", "SERIAL404"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    let failure = receiver.recv().await.unwrap();
    assert_failure_contains(
        failure,
        &command_id,
        "no configured Bambu printer matches serial SERIAL404",
    );
    assert!(!format!("{:?}", receiver).contains("ACCESS-CODE-SECRET"));
}

#[tokio::test]
async fn refresh_printer_materials_command_timeout_emits_ack_then_failure_without_material_snapshot()
 {
    let config = test_config();
    let gateway = FakeGateway::material_fail_with_access_code(
        "ACCESS-CODE-SECRET",
        anyhow::anyhow!("timed out waiting for MQTT report")
            .context("no AMS material report received before timeout"),
    );
    let (sender, mut receiver) = mpsc::channel(3);
    let command_id = uuid::Uuid::new_v4().to_string();

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        refresh_materials_command(command_id.clone(), "printer-1", "SERIAL1"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    let failure = receiver.recv().await.unwrap();
    assert_failure_contains(
        failure,
        &command_id,
        "no AMS material report received before timeout",
    );
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn refresh_printer_materials_command_works_for_runtime_linked_printer() {
    let config = test_config();
    let gateway = TestRuntimeBambuMachineGateway::new(
        Vec::new(),
        FakeMachineFileTransfer::default(),
        Duration::from_millis(50),
    );
    gateway
        .push_command_transport(FakeMqttTransport::with_reports([
            get_version_report("A1 Mini"),
            ams_ready_report("PLA"),
        ]))
        .await;
    gateway
        .set_discovered_printers(vec![DiscoveredPrinter {
            serial_number: Some("SERIAL123".to_owned()),
            host: "192.0.2.10".to_owned(),
            name: Some("garage".to_owned()),
            model: Some("A1 Mini".to_owned()),
            source: "ssdp",
        }])
        .await;
    let (sender, mut events) = mpsc::channel(8);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command(uuid::Uuid::new_v4().to_string(), "ACCESS-CODE-SECRET"),
    )
    .await
    .unwrap();
    drain_until_success(&mut events).await;

    let command_id = uuid::Uuid::new_v4().to_string();
    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        refresh_materials_command(command_id.clone(), "printer-1", "SERIAL123"),
    )
    .await
    .unwrap();

    assert_eq!(
        events.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_material_snapshot(events.recv().await.unwrap(), "SERIAL123", Some("printer-1"));
}

#[tokio::test]
async fn refresh_printer_materials_command_unknown_runtime_serial_fails_redacted() {
    let config = test_config();
    let gateway = TestRuntimeBambuMachineGateway::new(
        Vec::<(
            BambuPrinterEndpoint,
            FakeMqttTransport,
            FakeMachineFileTransfer,
        )>::new(),
        FakeMachineFileTransfer::default(),
        Duration::from_millis(50),
    );
    let (sender, mut events) = mpsc::channel(4);
    let command_id = uuid::Uuid::new_v4().to_string();

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        refresh_materials_command(command_id.clone(), "printer-1", "UNKNOWN"),
    )
    .await
    .unwrap();

    assert_eq!(
        events.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    let failure = events.recv().await.unwrap();
    assert_failure_contains(
        failure,
        &command_id,
        "no configured Bambu printer matches serial UNKNOWN",
    );
}
