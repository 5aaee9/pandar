use super::*;

#[tokio::test]
async fn firmware_runtime_existing_generation_link_retries_invalidation_before_modules() {
    let config = test_config();
    let old_endpoint = runtime_endpoint("SERIAL1", "old office", "ACCESS-1");
    let gateway = crate::machine::runtime::RuntimeBambuMachineGateway::new(
        config.clone(),
        vec![old_endpoint.clone()],
        Duration::from_secs(1),
    );
    let (seed_sender, mut seed_events) = mpsc::channel(4);
    let old_generation = gateway
        .firmware_cache()
        .begin_generation(&config, old_endpoint, &seed_sender, None)
        .await
        .unwrap()
        .unwrap()
        .generation();
    seed_events.recv().await.unwrap();

    let endpoint = runtime_endpoint("SERIAL1", "new office", "ACCESS-2");
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
                    device_features2: None,
                    nozzle_system: None,
                    telemetry_authoritative: true,
                },
                materials: None,
            },
            FirmwareVersionObservation {
                model: "P2S".into(),
                modules: vec![pandar_core::PrinterFirmwareModule {
                    name: "ota".into(),
                    software_version: Some("02.00".into()),
                    software_new_version: None,
                    new_version: None,
                    visible: None,
                    product_name: None,
                    serial_number: None,
                    hardware_version: None,
                    firmware_flag: None,
                }],
            },
        )))
        .await;
    let (sender, mut events) = mpsc::channel(16);

    gateway
        .link_printer(endpoint, &config, &sender)
        .await
        .unwrap();

    let mut ownership_events = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let event @ (agent_event::Event::PrinterSnapshot(_)
        | agent_event::Event::PrinterFirmwareInvalidated(_)
        | agent_event::Event::PrinterFirmwareModulesSnapshot(_)) = event.event.unwrap()
        {
            ownership_events.push(event);
        }
    }
    assert_eq!(ownership_events.len(), 4);
    let agent_event::Event::PrinterFirmwareInvalidated(initial) = &ownership_events[0] else {
        panic!("existing generation link must invalidate before its ownership snapshot");
    };
    assert_eq!(initial.generation, old_generation + 1);
    assert!(matches!(
        ownership_events[1],
        agent_event::Event::PrinterSnapshot(_)
    ));
    let agent_event::Event::PrinterFirmwareInvalidated(retry) = &ownership_events[2] else {
        panic!("existing generation link must retry its invalidation after the snapshot");
    };
    assert_eq!(retry.generation, initial.generation);
    let agent_event::Event::PrinterFirmwareModulesSnapshot(modules) = &ownership_events[3] else {
        panic!("link validation modules must follow the ownership retry");
    };
    assert_eq!(modules.generation, initial.generation);
    assert_eq!(modules.module_revision, 1);
}
