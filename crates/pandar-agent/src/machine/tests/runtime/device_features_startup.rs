use super::*;

#[tokio::test]
async fn device_features_session_startup_precedes_queued_command_and_refreshes_zero() {
    let transport = FakeMqttTransport::with_reports([
        runtime_feature_report("RUNNING", "8000004100000020"),
        get_version_report("X1 Carbon"),
        runtime_feature_report("READY", "0"),
    ]);
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
    let cache = gateway.device_feature_cache();
    cache
        .update(
            "SERIAL1",
            BambuDeviceFeatures::from_bits(DEVICE_FEATURE_HIGH_BITS),
        )
        .await;
    let config = test_config();
    let (sender, mut events) = mpsc::channel(16);
    sender.send(crate::hello_event(&config)).await.unwrap();
    let (commands_sender, commands_receiver) = mpsc::channel(1);
    commands_sender
        .send(Ok(HubCommand {
            command_id: "refresh-after-features".to_owned(),
            command: Some(hub_command::Command::RefreshPrinters(RefreshPrinters {})),
        }))
        .await
        .unwrap();
    let (command_release, released) = tokio::sync::oneshot::channel();

    let task = tokio::spawn({
        let gateway = std::sync::Arc::clone(&gateway);
        let config = config.clone();
        async move {
            gateway.prepare_session(&config, &sender).await?;
            released.await.expect("release queued Hub command");
            crate::handle_command_stream_with_gateway(
                &config,
                gateway,
                &sender,
                tokio_stream::wrappers::ReceiverStream::new(commands_receiver),
                1,
            )
            .await
        }
    });

    assert!(matches!(
        events.recv().await.unwrap().event,
        Some(agent_event::Event::Hello(_))
    ));
    assert_eq!(feature_event_bits(events.recv().await.unwrap()), None);
    assert_eq!(
        feature_event_bits(events.recv().await.unwrap()),
        Some(DEVICE_FEATURE_HIGH_BITS)
    );
    assert_eq!(
        cache.get("SERIAL1").await.unwrap().bits(),
        DEVICE_FEATURE_HIGH_BITS
    );
    command_release.send(()).unwrap();
    assert!(matches!(
        events.recv().await.unwrap().event,
        Some(agent_event::Event::CommandAck(_))
    ));
    let full_snapshot = events.recv().await.unwrap();
    let Some(agent_event::Event::PrinterSnapshot(full_snapshot)) = full_snapshot.event else {
        panic!("expected refreshed full printer snapshot");
    };
    assert_eq!(
        full_snapshot.device_features.unwrap().bambu_fun_bits,
        Some(0),
        "valid zero must overwrite the prior nonzero value"
    );
    assert!(matches!(
        events.recv().await.unwrap().event,
        Some(agent_event::Event::PrinterMaterialsSnapshot(_))
    ));
    assert!(matches!(
        events.recv().await.unwrap().event,
        Some(agent_event::Event::CommandResult(result)) if result.success
    ));
    assert_eq!(cache.get("SERIAL1").await.unwrap().bits(), 0);

    let published = transport.published_commands().await;
    assert_eq!(published[0].payload["pushing"]["command"], "pushall");
    assert_eq!(published[1].payload["info"]["command"], "get_version");
    assert_eq!(published[2].payload["pushing"]["command"], "pushall");
    task.abort();
}

#[tokio::test]
async fn device_features_session_startup_aborts_stale_report_cache_writer() {
    let transport = FakeMqttTransport::with_reports([runtime_fun_only_report("0")]);
    let transfer = FakeMachineFileTransfer::default();
    let gateway = std::sync::Arc::new(TestRuntimeBambuMachineGateway::new(
        vec![(
            runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
            transport,
            transfer.clone(),
        )],
        transfer,
        Duration::from_secs(1),
    ));
    let replacement_pause = gateway.pause_report_task_replacement().await;
    let release = std::sync::Arc::new(Notify::new());
    let cache = gateway.device_feature_cache();
    let stale_finished = install_stale_report_cache_write(
        &gateway.report_tasks,
        cache.clone(),
        "SERIAL1",
        BambuDeviceFeatures::from_bits(DEVICE_FEATURE_HIGH_BITS),
        std::sync::Arc::clone(&release),
    )
    .await;
    let (sender, mut events) = mpsc::channel(4);

    let prepare = tokio::spawn({
        let gateway = std::sync::Arc::clone(&gateway);
        async move { gateway.prepare_session(&test_config(), &sender).await }
    });
    replacement_pause.wait_until_blocked().await;
    assert_eq!(feature_event_bits(events.recv().await.unwrap()), None);
    assert_eq!(feature_event_bits(events.recv().await.unwrap()), Some(0));
    release.notify_waiters();
    tokio::task::yield_now().await;
    replacement_pause.release();
    prepare.await.unwrap().unwrap();

    assert!(stale_finished.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(cache.get("SERIAL1").await.unwrap().bits(), 0);
}

#[tokio::test]
async fn device_features_report_reconnect_invalidates_before_accepting_new_value() {
    let transport =
        FakeMqttTransport::with_receive_failure_then_reports([runtime_fun_only_report(
            "8000004100000020",
        )]);
    let cache = crate::machine::DeviceFeatureCache::default();
    cache
        .update("SERIAL1", BambuDeviceFeatures::from_bits(0x40))
        .await;
    let (sender, mut events) = mpsc::channel(4);
    let task = tokio::spawn(crate::machine::runtime::forward_print_reports_with_retry(
        test_config(),
        transport.clone(),
        runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
        Duration::from_secs(1),
        sender,
        Duration::from_millis(1),
        cache.clone(),
    ));

    assert_offline_event(events.recv().await.unwrap());
    assert_eq!(feature_event_bits(events.recv().await.unwrap()), None);
    assert_eq!(
        feature_event_bits(events.recv().await.unwrap()),
        Some(DEVICE_FEATURE_HIGH_BITS)
    );
    assert_eq!(
        cache.get("SERIAL1").await.unwrap().bits(),
        DEVICE_FEATURE_HIGH_BITS
    );
    assert_eq!(transport.subscribe_attempts().await, 2);
    let published = transport.published_commands().await;
    assert_eq!(published.len(), 2);
    assert_eq!(published[0].payload["pushing"]["command"], "pushall");
    assert_eq!(published[1].payload["pushing"]["command"], "pushall");
    task.abort();
}

#[tokio::test]
async fn device_features_report_failure_invalidates_before_retry_delay() {
    let transport = FakeMqttTransport::with_receive_failure_then_reports([]);
    let cache = crate::machine::DeviceFeatureCache::default();
    cache
        .update(
            "SERIAL1",
            BambuDeviceFeatures::from_bits(DEVICE_FEATURE_HIGH_BITS),
        )
        .await;
    let (sender, mut events) = mpsc::channel(2);
    let task = tokio::spawn(crate::machine::runtime::forward_print_reports_with_retry(
        test_config(),
        transport.clone(),
        runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
        Duration::from_secs(1),
        sender,
        Duration::from_secs(30),
        cache.clone(),
    ));

    let offline = tokio::time::timeout(Duration::from_millis(100), events.recv())
        .await
        .expect("failure should publish offline before the retry delay")
        .unwrap();
    assert_offline_event(offline);
    let invalidated = tokio::time::timeout(Duration::from_millis(100), events.recv())
        .await
        .expect("failure should invalidate features before the retry delay")
        .unwrap();
    assert_eq!(feature_event_bits(invalidated), None);
    assert_eq!(cache.get("SERIAL1").await, None);
    assert_eq!(transport.subscribe_attempts().await, 1);
    assert_eq!(transport.published_commands().await.len(), 1);
    task.abort();
}

#[tokio::test]
async fn device_features_idle_timeout_does_not_invalidate_or_reprobe() {
    let transport = FakeMqttTransport::with_timeout();
    let cache = crate::machine::DeviceFeatureCache::default();
    cache
        .update(
            "SERIAL1",
            BambuDeviceFeatures::from_bits(DEVICE_FEATURE_HIGH_BITS),
        )
        .await;
    let (sender, mut events) = mpsc::channel(2);
    let task = tokio::spawn(crate::machine::runtime::forward_print_reports_with_retry(
        test_config(),
        transport.clone(),
        runtime_endpoint("SERIAL1", "office", "ACCESS-1"),
        Duration::from_millis(1),
        sender,
        Duration::from_millis(1),
        cache.clone(),
    ));

    tokio::time::timeout(Duration::from_secs(1), async {
        while transport.published_commands().await.is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }

    assert_eq!(transport.subscribe_attempts().await, 1);
    assert_eq!(transport.published_commands().await.len(), 1);
    assert_eq!(
        cache.get("SERIAL1").await.unwrap().bits(),
        DEVICE_FEATURE_HIGH_BITS
    );
    assert_offline_event(events.recv().await.unwrap());
    assert!(events.try_recv().is_err());
    task.abort();
}
