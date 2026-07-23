use super::*;

mod existing_generation;

#[tokio::test]
async fn firmware_runtime_configured_startup_snapshot_precedes_first_generation() {
    let config = test_config();
    let mut endpoint = runtime_endpoint("SERIAL1", "configured office", "ACCESS-1");
    endpoint.host = "invalid host".into();
    let gateway = crate::machine::runtime::RuntimeBambuMachineGateway::new(
        config.clone(),
        vec![endpoint],
        Duration::from_millis(1),
    );
    let (sender, mut events) = mpsc::channel(16);

    gateway.prepare_session(&sender).await.unwrap();

    let first = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let event @ (agent_event::Event::PrinterSnapshot(_)
            | agent_event::Event::PrinterFirmwareInvalidated(_)) =
                events.recv().await.unwrap().event.unwrap()
            {
                break event;
            }
        }
    })
    .await
    .unwrap();
    let agent_event::Event::PrinterSnapshot(snapshot) = first else {
        panic!("configured startup must queue its printer row before firmware invalidation");
    };
    assert_eq!(snapshot.serial, "SERIAL1");
    assert_eq!(snapshot.name, "configured office");
    assert!(snapshot.state.is_empty());
    assert!(snapshot.connection_authoritative);
    assert!(!snapshot.telemetry_authoritative);

    let second = loop {
        if let event @ agent_event::Event::PrinterFirmwareInvalidated(_) =
            events.recv().await.unwrap().event.unwrap()
        {
            break event;
        }
    };
    assert!(matches!(
        second,
        agent_event::Event::PrinterFirmwareInvalidated(_)
    ));

    gateway.teardown_session_report_forwarders().await.unwrap();
    gateway.clear_session_sender(&sender).await;
}

#[tokio::test]
async fn firmware_runtime_cached_configured_printer_replays_session_ownership_before_generation() {
    let config = test_config();
    let mut endpoint = runtime_endpoint("SERIAL1", "configured office", "ACCESS-1");
    endpoint.host = "invalid host".into();
    let gateway = crate::machine::runtime::RuntimeBambuMachineGateway::new(
        config.clone(),
        vec![endpoint],
        Duration::from_millis(1),
    );
    let (lost_sender, _lost_events) = mpsc::channel(32);

    gateway.prepare_session(&lost_sender).await.unwrap();
    gateway.teardown_session_report_forwarders().await.unwrap();
    gateway.clear_session_sender(&lost_sender).await;
    assert!(gateway.firmware_cache().snapshot("SERIAL1").await.is_some());

    let (sender, mut events) = mpsc::channel(32);
    gateway.prepare_session(&sender).await.unwrap();

    let first = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let event @ (agent_event::Event::PrinterSnapshot(_)
            | agent_event::Event::PrinterFirmwareInvalidated(_)) =
                events.recv().await.unwrap().event.unwrap()
            {
                break event;
            }
        }
    })
    .await
    .unwrap();
    let agent_event::Event::PrinterSnapshot(snapshot) = first else {
        panic!("every reverse session must replay configured printer ownership before firmware");
    };
    assert_eq!(snapshot.serial, "SERIAL1");

    let invalidation = loop {
        if let agent_event::Event::PrinterFirmwareInvalidated(invalidation) =
            events.recv().await.unwrap().event.unwrap()
        {
            break invalidation;
        }
    };
    assert!(
        gateway
            .firmware_cache()
            .commit_report_modules(
                &config,
                "SERIAL1",
                invalidation.generation,
                vec![pandar_core::PrinterFirmwareModule {
                    name: "ota".into(),
                    software_version: Some("10.00".into()),
                    software_new_version: None,
                    new_version: None,
                    visible: None,
                    product_name: None,
                    serial_number: None,
                    hardware_version: None,
                    firmware_flag: None,
                }],
                &sender,
            )
            .await
            .unwrap()
    );
    let modules = loop {
        if let agent_event::Event::PrinterFirmwareModulesSnapshot(modules) =
            events.recv().await.unwrap().event.unwrap()
        {
            break modules;
        }
    };
    assert_eq!(modules.generation, invalidation.generation);
    assert_eq!(modules.module_revision, 1);
    assert_eq!(
        modules.modules[0].software_version.as_deref(),
        Some("10.00")
    );

    gateway.teardown_session_report_forwarders().await.unwrap();
    gateway.clear_session_sender(&sender).await;
}

#[tokio::test]
async fn firmware_runtime_link_joins_old_report_before_emitting_or_mutating() {
    let config = test_config();
    let old_endpoint = runtime_endpoint("SERIAL1", "old office", "ACCESS-1");
    let gateway = crate::machine::runtime::RuntimeBambuMachineGateway::new(
        config.clone(),
        vec![old_endpoint.clone()],
        Duration::from_secs(1),
    );
    let new_endpoint = runtime_endpoint("SERIAL1", "new office", "ACCESS-2");
    gateway
        .inject_link_validation_result_for_test(Ok((
            PrinterRefreshResult {
                snapshot: MachineSnapshot {
                    serial: new_endpoint.serial.clone(),
                    host: Some(new_endpoint.host.clone()),
                    access_code: Some(new_endpoint.access_code.clone()),
                    name: new_endpoint.name.clone().unwrap(),
                    model: Some("P2S".into()),
                    state: Some("READY".into()),
                    nozzle_temperatures: Vec::new(),
                    active_nozzle: None,
                    bed_temperature_celsius: None,
                    bed_target_temperature_celsius: None,
                    chamber_temperature_celsius: None,
                    chamber_target_temperature_celsius: None,
                    chamber_light_on: None,
                    device_features: None,
                    telemetry_authoritative: true,
                },
                materials: None,
            },
            FirmwareVersionObservation {
                model: "P2S".into(),
                modules: Vec::new(),
            },
        )))
        .await;
    gateway
        .install_panicking_report_forwarder_for_test("SERIAL1")
        .await;
    let (sender, mut events) = mpsc::channel(8);

    let error = gateway
        .link_printer(new_endpoint, &config, &sender)
        .await
        .unwrap_err();

    let message = format!("{error:#}");
    assert!(message.contains("join runtime printer report forwarder"));
    assert!(message.contains("firmware report forwarder panic sentinel"));
    assert!(
        events.try_recv().is_err(),
        "failed link must queue no event"
    );
    assert_eq!(
        gateway.camera_endpoint("SERIAL1").await.unwrap(),
        old_endpoint
    );
    assert!(gateway.firmware_cache().snapshot("SERIAL1").await.is_none());
}

#[tokio::test]
async fn firmware_runtime_link_waits_for_old_publish_transition_before_snapshot() {
    let config = test_config();
    let old_endpoint = runtime_endpoint("SERIAL1", "old office", "ACCESS-1");
    let gateway = std::sync::Arc::new(crate::machine::runtime::RuntimeBambuMachineGateway::new(
        config.clone(),
        vec![old_endpoint.clone()],
        Duration::from_secs(1),
    ));
    let cache = gateway.firmware_cache();
    let (seed_sender, mut seed_events) = mpsc::channel(4);
    let old_generation = cache
        .begin_generation(&config, old_endpoint, &seed_sender, None)
        .await
        .unwrap()
        .unwrap()
        .generation();
    seed_events.recv().await.unwrap();
    cache
        .prepare_firmware_control(FirmwarePrepareRequest {
            command_id: "old-publish".into(),
            serial: "SERIAL1".into(),
            expected_generation: old_generation,
            session_epoch: 1,
        })
        .await
        .unwrap();
    let execution = cache
        .claim_firmware_execute(&FirmwareExecuteRequest {
            command_id: "old-publish".into(),
            serial: "SERIAL1".into(),
            expected_generation: old_generation,
            session_epoch: 1,
            command: pandar_core::FirmwareCommand::UpgradeConfirm {
                sequence_id: "old-sequence".into(),
                src_id: 1,
            },
        })
        .await
        .unwrap();
    let publish_transition = execution.publish_transition().await.unwrap();
    let new_endpoint = runtime_endpoint("SERIAL1", "new office", "ACCESS-2");
    gateway
        .inject_link_validation_result_for_test(Ok((
            PrinterRefreshResult {
                snapshot: MachineSnapshot {
                    serial: new_endpoint.serial.clone(),
                    host: Some(new_endpoint.host.clone()),
                    access_code: Some(new_endpoint.access_code.clone()),
                    name: new_endpoint.name.clone().unwrap(),
                    model: Some("P2S".into()),
                    state: Some("READY".into()),
                    nozzle_temperatures: Vec::new(),
                    active_nozzle: None,
                    bed_temperature_celsius: None,
                    bed_target_temperature_celsius: None,
                    chamber_temperature_celsius: None,
                    chamber_target_temperature_celsius: None,
                    chamber_light_on: None,
                    device_features: None,
                    telemetry_authoritative: true,
                },
                materials: None,
            },
            FirmwareVersionObservation {
                model: "P2S".into(),
                modules: Vec::new(),
            },
        )))
        .await;
    let (sender, mut events) = mpsc::channel(8);
    let link = tokio::spawn({
        let gateway = std::sync::Arc::clone(&gateway);
        let config = config.clone();
        async move { gateway.link_printer(new_endpoint, &config, &sender).await }
    });

    assert!(
        tokio::time::timeout(Duration::from_millis(50), events.recv())
            .await
            .is_err(),
        "new snapshot must wait until the old publish transition is revoked"
    );

    drop(publish_transition);
    link.await.unwrap().unwrap();
    let first = events.recv().await.unwrap();
    assert!(matches!(
        first.event,
        Some(agent_event::Event::PrinterFirmwareInvalidated(_))
    ));
}

#[tokio::test]
async fn firmware_runtime_first_link_queues_snapshot_before_generation() {
    let config = test_config();
    let gateway = crate::machine::runtime::RuntimeBambuMachineGateway::new(
        config.clone(),
        Vec::new(),
        Duration::from_secs(1),
    );
    let endpoint = runtime_endpoint("SERIAL1", "new office", "ACCESS-1");
    gateway
        .inject_link_validation_result_for_test(Ok((
            PrinterRefreshResult {
                snapshot: MachineSnapshot {
                    serial: endpoint.serial.clone(),
                    host: Some(endpoint.host.clone()),
                    access_code: Some(endpoint.access_code.clone()),
                    name: endpoint.name.clone().unwrap(),
                    model: Some("P2S".into()),
                    state: Some("READY".into()),
                    nozzle_temperatures: Vec::new(),
                    active_nozzle: None,
                    bed_temperature_celsius: None,
                    bed_target_temperature_celsius: None,
                    chamber_temperature_celsius: None,
                    chamber_target_temperature_celsius: None,
                    chamber_light_on: None,
                    device_features: None,
                    telemetry_authoritative: true,
                },
                materials: None,
            },
            FirmwareVersionObservation {
                model: "P2S".into(),
                modules: Vec::new(),
            },
        )))
        .await;
    let (sender, mut events) = mpsc::channel(8);

    gateway
        .link_printer(endpoint, &config, &sender)
        .await
        .unwrap();

    let first = events.recv().await.unwrap();
    assert!(matches!(
        first.event,
        Some(agent_event::Event::PrinterSnapshot(_))
    ));
    let invalidation = events.recv().await.unwrap();
    let agent_event::Event::PrinterFirmwareInvalidated(invalidation) = invalidation.event.unwrap()
    else {
        panic!("expected first-link firmware invalidation after printer snapshot");
    };
    events.recv().await.unwrap();
    let modules = events.recv().await.unwrap();
    let agent_event::Event::PrinterFirmwareModulesSnapshot(modules) = modules.event.unwrap() else {
        panic!("expected first-link firmware modules after invalidation");
    };
    assert_eq!(modules.generation, invalidation.generation);
    assert_eq!(modules.module_revision, 1);
}
