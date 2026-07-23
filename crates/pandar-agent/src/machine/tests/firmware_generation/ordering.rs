use super::*;

#[tokio::test]
async fn firmware_generation_module_event_cannot_follow_new_invalidation() {
    let cache = FirmwareObservationCache::default();
    let config = test_config();
    let endpoint = endpoint("SERIAL1");
    let (sender, mut receiver) = mpsc::channel(8);
    let transition = cache
        .begin_generation(&config, endpoint.clone(), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation_one = transition.generation();
    drop(transition);
    assert_invalidated(receiver.recv().await.unwrap(), generation_one);

    let mut pause = firmware_event_pause::install("SERIAL1", FirmwareEventKind::Modules);
    let transport =
        crate::machine::mqtt::FakeMqttTransport::with_reports([version_report("old-generation")]);
    let task_config = config.clone();
    let task_transport = transport.clone();
    let task_endpoint = endpoint.clone();
    let task_sender = sender.clone();
    let task_cache = cache.clone();
    let report_task = tokio::spawn(async move {
        crate::machine::mqtt::forward_print_reports_with_firmware(
            &task_config,
            &task_transport,
            &task_endpoint,
            Duration::from_secs(1),
            &task_sender,
            &crate::machine::DeviceFeatureCache::default(),
            FirmwareReportContext {
                cache: task_cache,
                generation: generation_one,
            },
        )
        .await
    });
    pause.wait_until_reached().await;

    let transition_cache = cache.clone();
    let transition_config = config.clone();
    let transition_endpoint = endpoint.clone();
    let transition_sender = sender.clone();
    let transition_task = tokio::spawn(async move {
        transition_cache
            .begin_generation(
                &transition_config,
                transition_endpoint,
                &transition_sender,
                Some(generation_one),
            )
            .await
            .unwrap()
            .unwrap()
    });
    let early_offline = match tokio::time::timeout(Duration::from_millis(50), receiver.recv()).await
    {
        Ok(Some(event)) => {
            assert_mqtt_offline(event);
            true
        }
        Ok(None) => panic!("Agent event channel closed while modules were paused"),
        Err(_) => false,
    };
    pause.release();

    let old_event = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareModulesSnapshot(old_event) = old_event.event.unwrap()
    else {
        panic!("old module event must be queued before invalidation");
    };
    assert_eq!(old_event.generation, generation_one);
    let new_transition = transition_task.await.unwrap();
    let generation_two = new_transition.generation();
    drop(new_transition);
    if !early_offline {
        assert_mqtt_offline(receiver.recv().await.unwrap());
    }
    assert_invalidated(receiver.recv().await.unwrap(), generation_two);
    assert!(receiver.try_recv().is_err());
    report_task.abort();
}

#[tokio::test]
async fn firmware_generation_status_event_cannot_follow_new_invalidation() {
    let cache = FirmwareObservationCache::default();
    let config = test_config();
    let endpoint = endpoint("SERIAL1");
    let (sender, mut receiver) = mpsc::channel(8);
    let transition = cache
        .begin_generation(&config, endpoint.clone(), &sender, None)
        .await
        .unwrap()
        .unwrap();
    let generation_one = transition.generation();
    drop(transition);
    assert_invalidated(receiver.recv().await.unwrap(), generation_one);

    let mut pause = firmware_event_pause::install("SERIAL1", FirmwareEventKind::Status);
    let transport = crate::machine::mqtt::FakeMqttTransport::with_reports([serde_json::json!({
        "print": { "msg": 0, "upgrade_state": { "status": "OLD" } }
    })]);
    let task_config = config.clone();
    let task_transport = transport.clone();
    let task_endpoint = endpoint.clone();
    let task_sender = sender.clone();
    let task_cache = cache.clone();
    let report_task = tokio::spawn(async move {
        crate::machine::mqtt::forward_print_reports_with_firmware(
            &task_config,
            &task_transport,
            &task_endpoint,
            Duration::from_secs(1),
            &task_sender,
            &crate::machine::DeviceFeatureCache::default(),
            FirmwareReportContext {
                cache: task_cache,
                generation: generation_one,
            },
        )
        .await
    });
    pause.wait_until_reached().await;

    let transition_cache = cache.clone();
    let transition_config = config.clone();
    let transition_endpoint = endpoint.clone();
    let transition_sender = sender.clone();
    let transition_task = tokio::spawn(async move {
        transition_cache
            .begin_generation(
                &transition_config,
                transition_endpoint,
                &transition_sender,
                Some(generation_one),
            )
            .await
            .unwrap()
            .unwrap()
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(50), receiver.recv())
            .await
            .is_err(),
        "generation invalidation must wait until the old status event is enqueued"
    );
    pause.release();

    let old_event = next_firmware_event(&mut receiver).await;
    let agent_event::Event::PrinterFirmwareStatusSnapshot(old_event) = old_event.event.unwrap()
    else {
        panic!("old status event must be queued before invalidation");
    };
    assert_eq!(old_event.generation, generation_one);
    let new_transition = transition_task.await.unwrap();
    let generation_two = new_transition.generation();
    drop(new_transition);
    assert_mqtt_offline(receiver.recv().await.unwrap());
    assert_invalidated(receiver.recv().await.unwrap(), generation_two);
    assert!(receiver.try_recv().is_err());
    report_task.abort();
}
