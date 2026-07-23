use super::*;

#[tokio::test(start_paused = true)]
async fn periodic_printer_refresh_runtime_failure_retries_with_fresh_timer() {
    let config = test_config();
    let endpoint = endpoint();
    let firmware_cache = FirmwareObservationCache::default();
    let device_features = DeviceFeatureCache::default();
    device_features
        .update(&endpoint.serial, BambuDeviceFeatures::from_bits(0x40))
        .await;
    let (sender, mut receiver) = mpsc::channel(128);
    let transition = firmware_cache
        .begin_generation(&config, endpoint.clone(), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation_one = transition.generation();
    drop(transition);
    let initial_event = next_event(&mut receiver).await;
    let Some(agent_event::Event::PrinterFirmwareInvalidated(initial)) = initial_event.event else {
        panic!("initial generation must emit firmware invalidation");
    };
    assert_eq!(initial.serial, endpoint.serial);
    assert_eq!(initial.generation, generation_one);

    let transport = ControlledTransport::new(Some(3));
    let task_transport = transport.clone();
    let task_config = config.clone();
    let task_endpoint = endpoint.clone();
    let task_firmware_cache = firmware_cache.clone();
    let task_device_features = device_features.clone();
    let task = tokio::spawn(async move {
        crate::machine::runtime::forward_print_reports_with_firmware_retry(
            task_config,
            task_transport,
            task_endpoint,
            Duration::from_secs(10),
            sender,
            Duration::from_secs(5),
            RuntimeReportContext {
                device_features: task_device_features,
                firmware: FirmwareReportContext {
                    cache: task_firmware_cache,
                    generation: generation_one,
                },
            },
        )
        .await;
    });

    transport.wait_for_subscriptions(1).await;
    transport.wait_for_publish_attempts(2).await;
    transport.wait_for_report_waits(1).await;
    let initial_publishes = transport.published_commands();
    assert_eq!(initial_publishes[0].0, 1);
    assert_eq!(
        initial_publishes[0].1.payload["info"]["command"],
        "get_version"
    );
    assert_eq!(initial_publishes[1].0, 2);
    assert_eq!(
        initial_publishes[1].1.payload["pushing"]["command"],
        "pushall"
    );

    advance(Duration::from_secs(60)).await;
    transport.wait_for_publish_attempts(3).await;
    let failed_publish = &transport.published_commands()[2];
    assert_eq!(failed_publish.0, 3);
    assert_eq!(failed_publish.1.payload["pushing"]["command"], "pushall");
    let offline = next_snapshot(&mut receiver).await;
    assert_eq!(offline.state, "offline");
    assert!(!offline.telemetry_authoritative);
    let invalidation_event = next_event(&mut receiver).await;
    let Some(agent_event::Event::PrinterFirmwareInvalidated(invalidation)) =
        invalidation_event.event
    else {
        panic!("periodic failure must invalidate the firmware generation");
    };
    assert_eq!(invalidation.serial, endpoint.serial);
    assert_eq!(invalidation.generation, generation_one + 1);
    let generation_two = invalidation.generation;

    let feature_event = next_event(&mut receiver).await;
    let Some(agent_event::Event::PrinterDeviceFeaturesSnapshot(features)) = feature_event.event
    else {
        panic!("periodic failure must invalidate cached device features");
    };
    assert_eq!(features.serial, endpoint.serial);
    assert!(features.device_features.is_none());
    assert!(device_features.get(&endpoint.serial).await.is_none());
    assert_eq!(
        firmware_cache
            .snapshot(&endpoint.serial)
            .await
            .expect("new firmware generation remains active")
            .generation,
        generation_two
    );

    advance(Duration::from_secs(5) - Duration::from_nanos(1)).await;
    yield_now().await;
    assert_eq!(transport.subscription_count(), 1);
    assert_eq!(transport.publish_attempts(), 3);
    advance(Duration::from_nanos(1)).await;
    transport.wait_for_subscriptions(2).await;
    transport.wait_for_publish_attempts(5).await;
    transport.wait_for_report_waits(2).await;

    let retry_publishes = transport.published_commands();
    assert_eq!(retry_publishes[3].0, 4);
    assert_eq!(
        retry_publishes[3].1.payload["info"]["command"],
        "get_version"
    );
    assert_eq!(retry_publishes[4].0, 5);
    assert_eq!(
        retry_publishes[4].1.payload["pushing"]["command"],
        "pushall"
    );

    advance(Duration::from_secs(55)).await;
    yield_now().await;
    assert_eq!(transport.publish_attempts(), 5);
    advance(Duration::from_secs(5) - Duration::from_nanos(1)).await;
    yield_now().await;
    assert_eq!(transport.publish_attempts(), 5);
    advance(Duration::from_nanos(1)).await;
    transport.wait_for_publish_attempts(6).await;
    transport.wait_for_report_waits(3).await;
    let final_publishes = transport.published_commands();
    assert_eq!(final_publishes[5].0, 6);
    assert_eq!(
        final_publishes[5].1.payload["pushing"]["command"],
        "pushall"
    );
    assert_eq!(transport.subscription_count(), 2);

    drop(receiver);
    settle_until(
        || task.is_finished(),
        "retry wrapper must stop when the Agent event receiver closes",
    )
    .await;
    task.await.unwrap();
}
