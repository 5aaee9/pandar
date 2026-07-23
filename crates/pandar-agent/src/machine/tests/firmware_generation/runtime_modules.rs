use super::*;

#[tokio::test]
async fn runtime_report_later_module_observation_reacquires_serial_lease() {
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

    let transport =
        crate::machine::mqtt::FakeMqttTransport::with_reports([version_report("startup")]);
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
            Duration::from_secs(1),
            &task_sender,
            &crate::machine::DeviceFeatureCache::default(),
            FirmwareReportContext {
                cache: task_cache,
                generation,
            },
        )
        .await
    });

    let first = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareModulesSnapshot(first) = first.event.unwrap() else {
        panic!("expected startup modules");
    };
    assert_eq!(first.module_revision, 1);

    let other_observation = cache.version_observation_lease("SERIAL1").await;
    transport
        .push_report(version_report("long-stream-later"))
        .await;
    let offline = tokio::time::timeout(Duration::from_millis(50), receiver.recv())
        .await
        .expect("report exhaustion must emit an offline transition")
        .unwrap();
    assert_mqtt_offline(offline);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), receiver.recv())
            .await
            .is_err(),
        "later long-stream observation must wait for the same-serial coordinator"
    );
    let other = cache
        .commit_modules("SERIAL1", generation, vec![module("coordinated-other")])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(other.revision, 2);
    drop(other_observation);

    let later = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareModulesSnapshot(later) = later.event.unwrap() else {
        panic!("expected later long-stream modules");
    };
    assert_eq!(later.module_revision, 3);
    assert_eq!(
        later.modules[0].software_version.as_deref(),
        Some("long-stream-later")
    );
    task.abort();
}

#[tokio::test]
async fn runtime_report_present_empty_modules_replace_prior_snapshot() {
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

    let transport =
        crate::machine::mqtt::FakeMqttTransport::with_reports([version_report("prior")]);
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
            Duration::from_secs(1),
            &task_sender,
            &crate::machine::DeviceFeatureCache::default(),
            FirmwareReportContext {
                cache: task_cache,
                generation,
            },
        )
        .await
    });

    let first = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareModulesSnapshot(first) = first.event.unwrap() else {
        panic!("expected prior modules snapshot");
    };
    assert_eq!(first.module_revision, 1);

    transport
        .push_report(serde_json::json!({
            "info": { "command": "get_version", "module": [] }
        }))
        .await;
    let empty = tokio::time::timeout(
        Duration::from_millis(250),
        next_firmware_event(&mut receiver),
    )
    .await
    .expect("present-empty long-lived report must emit a replacement snapshot");
    let agent_event::Event::PrinterFirmwareModulesSnapshot(empty) = empty.event.unwrap() else {
        panic!("expected present-empty modules snapshot");
    };
    assert_eq!(empty.module_revision, 2);
    assert!(empty.modules.is_empty());
    let snapshot = cache.snapshot("SERIAL1").await.unwrap();
    assert_eq!(snapshot.module_revision, 2);
    assert_eq!(snapshot.modules, Some(Vec::new()));
    task.abort();
}

#[tokio::test]
async fn runtime_report_future_only_modules_replace_prior_snapshot() {
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

    let transport =
        crate::machine::mqtt::FakeMqttTransport::with_reports([version_report("prior")]);
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
            Duration::from_secs(1),
            &task_sender,
            &crate::machine::DeviceFeatureCache::default(),
            FirmwareReportContext {
                cache: task_cache,
                generation,
            },
        )
        .await
    });

    let first = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareModulesSnapshot(first) = first.event.unwrap() else {
        panic!("expected prior modules snapshot");
    };
    assert_eq!(first.module_revision, 1);

    transport
        .push_report(serde_json::json!({
            "info": {
                "command": "get_version",
                "module": [{ "name": "future/unit", "sw_ver": "future" }]
            }
        }))
        .await;
    let future = tokio::time::timeout(
        Duration::from_millis(250),
        next_firmware_event(&mut receiver),
    )
    .await
    .expect("future-only long-lived report must emit a replacement snapshot");
    let agent_event::Event::PrinterFirmwareModulesSnapshot(future) = future.event.unwrap() else {
        panic!("expected future-only modules snapshot");
    };
    assert_eq!(future.module_revision, 2);
    assert_eq!(future.modules.len(), 1);
    assert_eq!(future.modules[0].name, "future/unit");
    assert_eq!(
        future.modules[0].software_version.as_deref(),
        Some("future")
    );
    let snapshot = cache.snapshot("SERIAL1").await.unwrap();
    assert_eq!(snapshot.module_revision, 2);
    assert_eq!(snapshot.modules.unwrap()[0].name, "future/unit");
    task.abort();
}
