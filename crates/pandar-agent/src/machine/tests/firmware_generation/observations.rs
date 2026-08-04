use super::*;

#[tokio::test]
async fn firmware_generation_refresh_leases_serialize_per_serial_only() {
    let cache = FirmwareObservationCache::default();
    let first = cache.version_observation_lease("SERIAL1").await;
    let same_cache = cache.clone();
    let (same_acquired, same_receiver) = oneshot::channel();
    let same = tokio::spawn(async move {
        let _lease = same_cache.version_observation_lease("SERIAL1").await;
        let _ = same_acquired.send(());
    });

    assert!(
        tokio::time::timeout(Duration::from_millis(25), same_receiver)
            .await
            .is_err()
    );
    let other = tokio::time::timeout(
        Duration::from_millis(250),
        cache.version_observation_lease("SERIAL2"),
    )
    .await
    .expect("different serial remains concurrent");
    drop(other);
    drop(first);
    same.await.unwrap();
}

#[tokio::test]
async fn runtime_report_firmware_observations_emit_without_synthetic_job_reports() {
    let cache = FirmwareObservationCache::default();
    let config = test_config();
    let endpoint = endpoint("SERIAL1");
    let (sender, mut receiver) = mpsc::channel(16);
    let transition = cache
        .begin_generation(&config, endpoint.clone(), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation = transition.generation();
    drop(transition);
    assert_invalidated(receiver.recv().await.unwrap(), generation);

    let transport = crate::machine::mqtt::FakeMqttTransport::with_reports([
        serde_json::json!({
            "info": {
                "command": "get_version",
                "module": [{
                    "name": "ota",
                    "product_name": "X1 Carbon",
                    "sw_ver": "01.08.02.00"
                }]
            }
        }),
        serde_json::json!({
            "print": {
                "command": "push_status",
                "msg": 0,
                "cfg": "cfg-value",
                "upgrade_state": { "status": "UPGRADING", "progress": "1" }
            }
        }),
    ]);
    let task_config = config.clone();
    let task_transport = transport.clone();
    let task_endpoint = endpoint.clone();
    let task_sender = sender.clone();
    let task_cache = cache.clone();
    let task = tokio::spawn(async move {
        crate::machine::mqtt::forward_print_reports_with_firmware(
            &task_config,
            &task_transport,
            &task_endpoint,
            Duration::from_millis(10),
            &task_sender,
            &crate::machine::DeviceFeatureCache::default(),
            FirmwareReportContext {
                cache: task_cache,
                generation,
            },
        )
        .await
    });

    let modules = receiver.recv().await.unwrap();
    let agent_event::Event::PrinterFirmwareModulesSnapshot(modules) = modules.event.unwrap() else {
        panic!("expected modules snapshot before ordinary report events");
    };
    assert_eq!(modules.generation, generation);
    assert_eq!(modules.module_revision, 1);
    assert_eq!(
        modules.modules[0].software_version.as_deref(),
        Some("01.08.02.00")
    );

    let status = receiver.recv().await.unwrap();
    let agent_event::Event::PrinterFirmwareStatusSnapshot(status) = status.event.unwrap() else {
        panic!("expected firmware status without a synthetic print report");
    };
    assert_eq!(status.generation, generation);
    assert_eq!(status.status_revision, 1);
    assert_eq!(status.cfg.as_deref(), Some("cfg-value"));
    assert_eq!(
        status.upgrade_state.unwrap().status.as_deref(),
        Some("UPGRADING")
    );
    let full_snapshot = receiver.recv().await.unwrap();
    let agent_event::Event::PrinterSnapshot(full_snapshot) = full_snapshot.event.unwrap() else {
        panic!("expected full MQTT status snapshot before offline transition");
    };
    assert!(full_snapshot.telemetry_authoritative);
    assert!(full_snapshot.state.is_empty());
    assert_mqtt_offline(receiver.recv().await.unwrap());
    assert!(receiver.try_recv().is_err());

    let published = transport.published_commands().await;
    assert_eq!(published.len(), 2);
    assert_eq!(published[0].payload["info"]["command"], "get_version");
    assert_eq!(published[1].payload["pushing"]["command"], "pushall");
    task.abort();
}

#[tokio::test]
async fn runtime_report_reconnect_establishes_new_generation_before_new_snapshots() {
    let cache = FirmwareObservationCache::default();
    let config = test_config();
    let endpoint = endpoint("SERIAL1");
    let (sender, mut receiver) = mpsc::channel(16);
    let transition = cache
        .begin_generation(&config, endpoint.clone(), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation_one = transition.generation();
    drop(transition);
    assert_invalidated(receiver.recv().await.unwrap(), generation_one);

    let transport = crate::machine::mqtt::FakeMqttTransport::with_receive_failure_then_reports([
        serde_json::json!({
            "info": {
                "command": "get_version",
                "module": [{ "name": "ota", "product_name": "X1", "sw_ver": "2" }]
            }
        }),
        serde_json::json!({
            "print": { "msg": 0, "upgrade_state": { "status": "UPGRADING" } }
        }),
    ]);
    let task_config = config.clone();
    let task_transport = transport.clone();
    let task_endpoint = endpoint.clone();
    let task_sender = sender.clone();
    let task_cache = cache.clone();
    let task = tokio::spawn(async move {
        crate::machine::runtime::forward_print_reports_with_firmware_retry(
            task_config,
            task_transport,
            task_endpoint,
            Duration::from_millis(10),
            task_sender,
            Duration::from_millis(1),
            RuntimeReportContext {
                device_features: crate::machine::DeviceFeatureCache::default(),
                firmware: FirmwareReportContext {
                    cache: task_cache,
                    generation: generation_one,
                },
            },
        )
        .await
    });

    let invalidation = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareInvalidated(invalidation) = invalidation.event.unwrap()
    else {
        panic!("reconnect must invalidate before snapshots");
    };
    let generation_two = invalidation.generation;
    assert!(generation_two > generation_one);

    let modules = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareModulesSnapshot(modules) = modules.event.unwrap() else {
        panic!("expected new-generation modules after invalidation");
    };
    assert_eq!(modules.generation, generation_two);
    let status = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareStatusSnapshot(status) = status.event.unwrap() else {
        panic!("expected new-generation status after modules");
    };
    assert_eq!(status.generation, generation_two);
    assert_eq!(
        cache.snapshot("SERIAL1").await.unwrap().generation,
        generation_two
    );
    task.abort();
}

#[tokio::test]
async fn runtime_report_idle_timeout_releases_version_observation_lease() {
    let cache = FirmwareObservationCache::default();
    let config = test_config();
    let endpoint = endpoint("SERIAL1");
    let (sender, mut receiver) = mpsc::channel(8);
    let transition = cache
        .begin_generation(&config, endpoint.clone(), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation = transition.generation();
    drop(transition);
    assert_invalidated(receiver.recv().await.unwrap(), generation);

    let transport = crate::machine::mqtt::FakeMqttTransport::with_timeout();
    let task_config = config.clone();
    let task_transport = transport.clone();
    let task_endpoint = endpoint.clone();
    let task_sender = sender.clone();
    let task_cache = cache.clone();
    let task = tokio::spawn(async move {
        crate::machine::mqtt::forward_print_reports_with_firmware(
            &task_config,
            &task_transport,
            &task_endpoint,
            Duration::from_millis(10),
            &task_sender,
            &crate::machine::DeviceFeatureCache::default(),
            FirmwareReportContext {
                cache: task_cache,
                generation,
            },
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        while transport.published_commands().await.len() < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;

    let lease = tokio::time::timeout(
        Duration::from_millis(100),
        cache.version_observation_lease("SERIAL1"),
    )
    .await
    .expect("idle report timeout must release the startup version observation lease");
    drop(lease);
    task.abort();
}
