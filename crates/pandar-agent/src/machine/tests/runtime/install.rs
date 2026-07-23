use super::*;

#[tokio::test]
async fn runtime_install_keeps_lock_until_report_task_replacement_finishes() {
    let gateway = std::sync::Arc::new(TestRuntimeBambuMachineGateway::new(
        Vec::new(),
        FakeMachineFileTransfer::default(),
        Duration::from_secs(1),
    ));
    gateway
        .push_command_transport(runtime_transport([("X1 Carbon", "READY")]))
        .await;
    let pause = gateway.pause_report_task_replacement().await;
    let (sender, _) = mpsc::channel(1);
    let config = test_config();

    let link_gateway = std::sync::Arc::clone(&gateway);
    let link_sender = sender.clone();
    let link_config = config.clone();
    let link = tokio::spawn(async move {
        link_gateway
            .link_printer(
                runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
                &link_config,
                &link_sender,
            )
            .await
    });
    pause.wait_until_blocked().await;

    assert_locked_for_a_moment(&gateway).await.unwrap();

    pause.release();
    link.await.unwrap().unwrap();
}

#[tokio::test]
async fn report_forwarding_preparation_failure_leaves_previous_endpoint_active() {
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
        .push_command_transport(runtime_transport([("P2S", "RUNNING")]))
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
        .push_report_preparation_error(anyhow::anyhow!("prepare report transport failed"))
        .await;
    let err = gateway
        .link_printer(
            runtime_endpoint("SERIAL1", "new office", "ACCESS-2"),
            &test_config(),
            &sender,
        )
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("prepare report transport failed"));
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
