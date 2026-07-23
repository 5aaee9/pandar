use super::*;

#[tokio::test]
async fn device_features_endpoint_replacement_link_snapshot_precedes_feature_invalidation() {
    let transfer = FakeMachineFileTransfer::default();
    let gateway = std::sync::Arc::new(TestRuntimeBambuMachineGateway::new(
        vec![(
            runtime_endpoint("SERIAL1", "old office", "ACCESS-1"),
            PausedMqttTransport::ready("X1 Carbon", "READY"),
            transfer.clone(),
        )],
        transfer,
        Duration::from_secs(1),
    ));
    let replacement = PausedMqttTransport::new_with_feature("0");
    gateway.push_command_transport(replacement.clone()).await;
    let replacement_pause = gateway.pause_report_task_replacement().await;
    let cache = gateway.device_feature_cache();
    cache
        .update(
            "SERIAL1",
            BambuDeviceFeatures::from_bits(DEVICE_FEATURE_HIGH_BITS),
        )
        .await;
    let stale_release = std::sync::Arc::new(Notify::new());
    let stale_finished = install_stale_report_cache_write(
        &gateway.report_tasks,
        cache.clone(),
        "SERIAL1",
        BambuDeviceFeatures::from_bits(DEVICE_FEATURE_HIGH_BITS),
        std::sync::Arc::clone(&stale_release),
    )
    .await;
    let (sender, mut events) = mpsc::channel(4);
    let config = test_config();
    let link = tokio::spawn({
        let gateway = std::sync::Arc::clone(&gateway);
        async move {
            gateway
                .link_printer(
                    runtime_endpoint("SERIAL1", "new office", "ACCESS-2"),
                    &config,
                    &sender,
                )
                .await
        }
    });

    replacement.wait_until_blocked().await;
    assert_eq!(
        cache.get("SERIAL1").await.unwrap().bits(),
        DEVICE_FEATURE_HIGH_BITS
    );
    assert!(events.try_recv().is_err());

    replacement.release();
    replacement_pause.wait_until_blocked().await;
    stale_release.notify_waiters();
    tokio::task::yield_now().await;
    replacement_pause.release();
    let snapshot = link.await.unwrap().unwrap();
    let linked = events.recv().await.unwrap();
    let Some(agent_event::Event::PrinterSnapshot(linked)) = linked.event else {
        panic!("expected linked printer snapshot before feature invalidation");
    };
    assert_eq!(linked.serial, "SERIAL1");
    assert_eq!(feature_event_bits(events.recv().await.unwrap()), None);
    assert!(stale_finished.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(snapshot.device_features.unwrap().bits(), 0);
    assert_eq!(cache.get("SERIAL1").await.unwrap().bits(), 0);
}

#[tokio::test]
async fn device_features_invalid_refresh_keeps_cached_value() {
    let transport = FakeMqttTransport::with_reports([
        get_version_report("X1 Carbon"),
        runtime_feature_report("RUNNING", "not-hex"),
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
    let cache = gateway.device_feature_cache();
    cache
        .update(
            "SERIAL1",
            BambuDeviceFeatures::from_bits(DEVICE_FEATURE_HIGH_BITS),
        )
        .await;

    let snapshot = gateway.refresh_printers().await.unwrap().remove(0).snapshot;

    assert_eq!(snapshot.state.as_deref(), Some("RUNNING"));
    assert_eq!(snapshot.device_features, None);
    assert_eq!(
        cache.get("SERIAL1").await.unwrap().bits(),
        DEVICE_FEATURE_HIGH_BITS
    );
}
