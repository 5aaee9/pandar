use super::*;

#[tokio::test]
async fn firmware_generation_runtime_link_validation_allows_different_serials() {
    let gateway = std::sync::Arc::new(TestRuntimeBambuMachineGateway::new(
        Vec::new(),
        FakeMachineFileTransfer::default(),
        Duration::from_secs(1),
    ));
    let first_transport = PausedMqttTransport::new();
    let second_transport = PausedMqttTransport::new();
    gateway
        .push_command_transport(first_transport.clone())
        .await;
    gateway
        .push_command_transport(second_transport.clone())
        .await;
    let (sender, _events) = mpsc::channel(8);
    let config = test_config();

    let first_gateway = std::sync::Arc::clone(&gateway);
    let first_sender = sender.clone();
    let first_config = config.clone();
    let first = tokio::spawn(async move {
        first_gateway
            .link_printer(
                runtime_endpoint("SERIAL1", "first", "ACCESS-1"),
                &first_config,
                &first_sender,
            )
            .await
    });
    first_transport.wait_until_blocked().await;

    let second_gateway = std::sync::Arc::clone(&gateway);
    let second_sender = sender.clone();
    let second_config = config.clone();
    let second = tokio::spawn(async move {
        second_gateway
            .link_printer(
                runtime_endpoint("SERIAL2", "second", "ACCESS-2"),
                &second_config,
                &second_sender,
            )
            .await
    });
    tokio::time::timeout(
        Duration::from_millis(100),
        second_transport.wait_until_blocked(),
    )
    .await
    .expect("different serial validation must not wait on the global runtime gateway mutex");

    first_transport.release();
    second_transport.release();
    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();
    assert_eq!(gateway.report_task_count("SERIAL1").await, 1);
    assert_eq!(gateway.report_task_count("SERIAL2").await, 1);
}

#[tokio::test]
async fn firmware_generation_runtime_refresh_waits_for_same_serial_version_lease() {
    let transport = PausedMqttTransport::new();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = std::sync::Arc::new(TestRuntimeBambuMachineGateway::new(
        vec![(
            runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
            transport.clone(),
            transfer.clone(),
        )],
        transfer,
        Duration::from_secs(1),
    ));
    let cache = gateway.firmware_cache();
    let lease = cache.version_observation_lease("SERIAL1").await;
    let refresh_gateway = std::sync::Arc::clone(&gateway);
    let refresh = tokio::spawn(async move { refresh_gateway.refresh_printers().await });

    assert!(
        tokio::time::timeout(Duration::from_millis(50), transport.wait_until_blocked())
            .await
            .is_err(),
        "runtime refresh must obtain the same-serial version observation lease"
    );
    drop(lease);
    transport.wait_until_blocked().await;
    transport.release();
    refresh.await.unwrap().unwrap();
}

#[tokio::test]
async fn firmware_generation_runtime_refresh_allows_different_serials() {
    let first_transport = PausedMqttTransport::new();
    let second_transport = PausedMqttTransport::new();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = std::sync::Arc::new(TestRuntimeBambuMachineGateway::new(
        vec![
            (
                runtime_endpoint("SERIAL1", "first", "ACCESS-1"),
                first_transport.clone(),
                transfer.clone(),
            ),
            (
                runtime_endpoint("SERIAL2", "second", "ACCESS-2"),
                second_transport.clone(),
                transfer.clone(),
            ),
        ],
        transfer,
        Duration::from_secs(1),
    ));
    let refresh_gateway = std::sync::Arc::clone(&gateway);
    let refresh = tokio::spawn(async move { refresh_gateway.refresh_printers().await });

    first_transport.wait_until_blocked().await;
    tokio::time::timeout(
        Duration::from_millis(100),
        second_transport.wait_until_blocked(),
    )
    .await
    .expect("different serial refresh must not wait on another serial's MQTT observation");
    first_transport.release();
    second_transport.release();
    let results = refresh.await.unwrap().unwrap();
    assert_eq!(
        results
            .into_iter()
            .map(|result| result.snapshot.serial)
            .collect::<Vec<_>>(),
        ["SERIAL1", "SERIAL2"]
    );
}

#[tokio::test]
async fn firmware_generation_runtime_refresh_commits_same_query_modules() {
    let transport = FakeMqttTransport::with_reports([
        serde_json::json!({
            "info": {
                "command": "get_version",
                "module": [
                    { "name": "ota", "product_name": "X1 Carbon", "sw_ver": "ota-1" },
                    { "name": "ams/0", "sw_ver": "ams-1", "hw_ver": "A00" }
                ]
            }
        }),
        runtime_state_report("READY"),
    ]);
    let transfer = FakeMachineFileTransfer::default();
    let gateway = TestRuntimeBambuMachineGateway::new(
        vec![(
            runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
            transport,
            transfer.clone(),
        )],
        transfer,
        Duration::from_secs(1),
    );
    let config = test_config();
    let (sender, mut events) = mpsc::channel(8);
    let cache = gateway.firmware_cache();
    let transition = cache
        .begin_generation(
            &config,
            runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
            &sender,
            None,
        )
        .await
        .unwrap()
        .unwrap();
    let generation = transition.generation();
    drop(transition);
    assert!(matches!(
        events.recv().await.unwrap().event,
        Some(agent_event::Event::PrinterFirmwareInvalidated(_))
    ));
    gateway.set_refresh_context(config, sender).await;

    let results = gateway.refresh_printers().await.unwrap();
    assert_eq!(results[0].snapshot.model.as_deref(), Some("X1 Carbon"));
    let event = tokio::time::timeout(Duration::from_millis(100), events.recv())
        .await
        .expect("runtime refresh must emit modules from its get_version response")
        .unwrap();
    let agent_event::Event::PrinterFirmwareModulesSnapshot(modules) = event.event.unwrap() else {
        panic!("expected firmware modules snapshot");
    };
    assert_eq!(modules.generation, generation);
    assert_eq!(modules.module_revision, 1);
    assert_eq!(
        modules
            .modules
            .iter()
            .map(|module| module.name.as_str())
            .collect::<Vec<_>>(),
        ["ota", "ams/0"]
    );
    let snapshot = cache.snapshot("SERIAL1").await.unwrap();
    assert_eq!(snapshot.module_revision, 1);
    assert_eq!(snapshot.modules.unwrap().len(), 2);
}
