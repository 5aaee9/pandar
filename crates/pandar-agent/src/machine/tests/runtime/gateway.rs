use super::*;

#[tokio::test]
async fn report_forwarder_retries_initial_subscribe_failure() {
    let transport = FakeMqttTransport::with_subscribe_failures(1);
    let (sender, mut receiver) = mpsc::channel(1);
    let task = tokio::spawn(crate::machine::runtime::forward_print_reports_with_retry(
        test_config(),
        transport.clone(),
        runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
        Duration::from_secs(1),
        sender,
        Duration::from_millis(1),
        crate::machine::DeviceFeatureCache::default(),
    ));

    assert_offline_event(receiver.recv().await.unwrap());
    assert_eq!(feature_event_bits(receiver.recv().await.unwrap()), None);

    tokio::time::timeout(Duration::from_secs(1), async {
        while transport.subscribe_attempts().await < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    task.abort();
}

#[tokio::test]
async fn empty_runtime_gateway_refresh_printers_returns_empty() {
    let gateway = TestRuntimeBambuMachineGateway::new(
        Vec::<(
            BambuPrinterEndpoint,
            FakeMqttTransport,
            FakeMachineFileTransfer,
        )>::new(),
        FakeMachineFileTransfer::default(),
        Duration::from_secs(1),
    );

    assert_eq!(gateway.refresh_printers().await.unwrap(), Vec::new());
}

#[tokio::test]
async fn successful_link_printer_installs_endpoint_for_later_refresh() {
    let gateway = TestRuntimeBambuMachineGateway::new(
        Vec::new(),
        FakeMachineFileTransfer::default(),
        Duration::from_secs(1),
    );
    gateway
        .push_command_transport(runtime_transport([
            ("X1 Carbon", "READY"),
            ("X1 Carbon", "IDLE"),
        ]))
        .await;
    let (sender, _) = mpsc::channel(1);

    let snapshot = gateway
        .link_printer(
            runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
            &test_config(),
            &sender,
        )
        .await
        .unwrap();

    assert_eq!(snapshot.state.as_deref(), Some("READY"));
    assert_eq!(gateway.report_task_count("SERIAL1").await, 1);
    assert_eq!(
        gateway
            .refresh_printers()
            .await
            .unwrap()
            .into_iter()
            .map(|result| result.snapshot)
            .collect::<Vec<_>>(),
        vec![MachineSnapshot {
            serial: "SERIAL1".to_string(),
            host: Some("192.0.2.10".to_string()),
            access_code: Some("ACCESS-1".to_string()),
            name: "office".to_string(),
            model: Some("X1 Carbon".to_string()),
            state: Some("IDLE".to_string()),
            nozzle_temperatures: Vec::new(),
            active_nozzle: None,
            bed_temperature_celsius: None,
            bed_target_temperature_celsius: None,
            chamber_temperature_celsius: None,
            chamber_target_temperature_celsius: None,
            chamber_light_on: None,
            device_features: None,
            telemetry_authoritative: true,
        }]
    );
}

#[tokio::test]
async fn same_serial_replacement_after_validation_success_leaves_one_report_task() {
    let gateway = TestRuntimeBambuMachineGateway::new(
        Vec::new(),
        FakeMachineFileTransfer::default(),
        Duration::from_secs(1),
    );
    gateway
        .push_command_transport(runtime_transport([("X1 Carbon", "READY")]))
        .await;
    gateway
        .push_command_transport(runtime_transport([("P2S", "RUNNING"), ("P2S", "PAUSED")]))
        .await;
    let (sender, _) = mpsc::channel(1);

    gateway
        .link_printer(
            runtime_endpoint("SERIAL1", "old office", "ACCESS-1"),
            &test_config(),
            &sender,
        )
        .await
        .unwrap();
    gateway
        .link_printer(
            runtime_endpoint("SERIAL1", "new office", "ACCESS-2"),
            &test_config(),
            &sender,
        )
        .await
        .unwrap();

    assert_eq!(gateway.report_task_count("SERIAL1").await, 1);
    assert_eq!(
        gateway
            .refresh_printers()
            .await
            .unwrap()
            .into_iter()
            .map(|result| result.snapshot)
            .collect::<Vec<_>>(),
        vec![MachineSnapshot {
            serial: "SERIAL1".to_string(),
            host: Some("192.0.2.10".to_string()),
            access_code: Some("ACCESS-2".to_string()),
            name: "new office".to_string(),
            model: Some("P2S".to_string()),
            state: Some("PAUSED".to_string()),
            nozzle_temperatures: Vec::new(),
            active_nozzle: None,
            bed_temperature_celsius: None,
            bed_target_temperature_celsius: None,
            chamber_temperature_celsius: None,
            chamber_target_temperature_celsius: None,
            chamber_light_on: None,
            device_features: None,
            telemetry_authoritative: true,
        }]
    );
}

#[tokio::test]
async fn same_serial_replacement_after_validation_failure_leaves_previous_endpoint_active() {
    let gateway = TestRuntimeBambuMachineGateway::new(
        Vec::new(),
        FakeMachineFileTransfer::default(),
        Duration::from_secs(1),
    );
    gateway
        .push_command_transport(runtime_transport([
            ("X1 Carbon", "READY"),
            ("X1 Carbon", "IDLE"),
        ]))
        .await;
    gateway
        .push_command_transport(FakeMqttTransport::with_timeout())
        .await;
    let (sender, _) = mpsc::channel(1);

    gateway
        .link_printer(
            runtime_endpoint("SERIAL1", "old office", "ACCESS-1"),
            &test_config(),
            &sender,
        )
        .await
        .unwrap();
    let err = gateway
        .link_printer(
            runtime_endpoint("SERIAL1", "new office", "ACCESS-2"),
            &test_config(),
            &sender,
        )
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("validate runtime printer SERIAL1"));
    assert_eq!(gateway.report_task_count("SERIAL1").await, 1);
    assert_eq!(
        gateway
            .refresh_printers()
            .await
            .unwrap()
            .into_iter()
            .map(|result| result.snapshot)
            .collect::<Vec<_>>(),
        vec![MachineSnapshot {
            serial: "SERIAL1".to_string(),
            host: Some("192.0.2.10".to_string()),
            access_code: Some("ACCESS-1".to_string()),
            name: "old office".to_string(),
            model: Some("X1 Carbon".to_string()),
            state: Some("IDLE".to_string()),
            nozzle_temperatures: Vec::new(),
            active_nozzle: None,
            bed_temperature_celsius: None,
            bed_target_temperature_celsius: None,
            chamber_temperature_celsius: None,
            chamber_target_temperature_celsius: None,
            chamber_light_on: None,
            device_features: None,
            telemetry_authoritative: true,
        }]
    );
}

#[tokio::test]
async fn concurrent_same_serial_link_printer_calls_are_serialized() {
    let gateway = std::sync::Arc::new(TestRuntimeBambuMachineGateway::new(
        Vec::new(),
        FakeMachineFileTransfer::default(),
        Duration::from_secs(1),
    ));
    let paused = PausedMqttTransport::new();
    gateway.push_command_transport(paused.clone()).await;
    gateway
        .push_command_transport(PausedMqttTransport::ready("P2S", "IDLE"))
        .await;
    let (sender, _) = mpsc::channel(1);
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
    paused.wait_until_blocked().await;
    assert_unlocked_for_a_moment(&gateway).await.unwrap();

    let second_gateway = std::sync::Arc::clone(&gateway);
    let second_sender = sender.clone();
    let second_config = config.clone();
    let second = tokio::spawn(async move {
        second_gateway
            .link_printer(
                runtime_endpoint("SERIAL1", "second", "ACCESS-2"),
                &second_config,
                &second_sender,
            )
            .await
    });
    tokio::task::yield_now().await;
    assert!(!second.is_finished());

    paused.release();
    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();
    assert_eq!(gateway.report_task_count("SERIAL1").await, 1);
}
