use super::*;

#[tokio::test]
async fn configured_print_project_file_rejects_unknown_flow_cali_before_upload() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let mut endpoint = endpoint("SERIAL1");
    endpoint.model = None;
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint, mqtt.clone(), transfer.clone())],
        Duration::from_secs(1),
    );
    let mut command = print_project_file();
    command.options.as_mut().unwrap().flow_cali = true;

    let err = gateway
        .print_project_file("SERIAL1", &command, b"abc".to_vec())
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("flow calibration"));
    assert!(transfer.recorded_requests().is_empty());
    assert!(mqtt.published_commands().await.is_empty());
}

#[tokio::test]
async fn configured_print_project_file_accepts_supported_flow_calibration_models() {
    for model in ["N6", "N7", "A1"] {
        let mqtt = FakeMqttTransport::default();
        let transfer = FakeMachineFileTransfer::default();
        let mut printer = endpoint("SERIAL1");
        printer.model = Some(model.to_owned());
        let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
            vec![(printer, mqtt.clone(), transfer.clone())],
            Duration::from_secs(1),
        );
        let mut command = print_project_file();
        command.options.as_mut().unwrap().flow_cali = true;
        command.options.as_mut().unwrap().auto_flow_cali = Some(1);

        gateway
            .print_project_file("SERIAL1", &command, b"abc".to_vec())
            .await
            .unwrap();

        assert!(!transfer.recorded_requests().is_empty(), "{model}");
        assert!(!mqtt.published_commands().await.is_empty(), "{model}");
    }
}

#[tokio::test]
async fn configured_print_project_file_accepts_n6_auto_flow_calibration() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let mut printer = endpoint("SERIAL1");
    printer.model = Some("N6".to_owned());
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(printer, mqtt.clone(), transfer.clone())],
        Duration::from_secs(1),
    );
    let mut command = print_project_file();
    command.options.as_mut().unwrap().flow_cali = false;
    command.options.as_mut().unwrap().auto_flow_cali = Some(2);

    gateway
        .print_project_file("SERIAL1", &command, b"abc".to_vec())
        .await
        .unwrap();

    assert!(!transfer.recorded_requests().is_empty());
    assert!(!mqtt.published_commands().await.is_empty());
}
#[tokio::test]
async fn configured_print_project_file_rejects_p1_flow_cali_before_upload() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let mut printer = endpoint("SERIAL1");
    printer.model = Some("C11".to_owned());
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(printer, mqtt.clone(), transfer.clone())],
        Duration::from_secs(1),
    );
    let mut command = print_project_file();
    command.options.as_mut().unwrap().flow_cali = true;
    command.options.as_mut().unwrap().auto_flow_cali = Some(1);

    let err = gateway
        .print_project_file("SERIAL1", &command, b"abc".to_vec())
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("flow calibration"));
    assert!(transfer.recorded_requests().is_empty());
    assert!(mqtt.published_commands().await.is_empty());
}

#[tokio::test]
async fn configured_print_project_file_rejects_a1_auto_flow_before_upload() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer.clone())],
        Duration::from_secs(1),
    );
    let mut command = print_project_file();
    command.options.as_mut().unwrap().auto_flow_cali = Some(2);

    let err = gateway
        .print_project_file("SERIAL1", &command, b"abc".to_vec())
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("automatic flow calibration"));
    assert!(transfer.recorded_requests().is_empty());
    assert!(mqtt.published_commands().await.is_empty());
}

#[tokio::test]
async fn configured_print_project_file_accepts_auto_bed_for_n7() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let mut printer = endpoint("SERIAL1");
    printer.model = Some("N7".to_owned());
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(printer, mqtt.clone(), transfer.clone())],
        Duration::from_secs(1),
    );
    let mut command = print_project_file();
    command.options.as_mut().unwrap().auto_bed_leveling = Some(2);

    gateway
        .print_project_file("SERIAL1", &command, b"abc".to_vec())
        .await
        .unwrap();

    assert!(!transfer.recorded_requests().is_empty());
    assert!(!mqtt.published_commands().await.is_empty());
}

#[tokio::test]
async fn configured_print_project_file_rejects_auto_bed_for_unsupported_models_before_upload() {
    for model in [Some("P1S"), Some("A1"), Some("Mystery Model"), None] {
        let mqtt = FakeMqttTransport::default();
        let transfer = FakeMachineFileTransfer::default();
        let mut printer = endpoint("SERIAL1");
        printer.model = model.map(str::to_owned);
        let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
            vec![(printer, mqtt.clone(), transfer.clone())],
            Duration::from_secs(1),
        );
        let mut command = print_project_file();
        command.options.as_mut().unwrap().auto_bed_leveling = Some(2);

        let err = gateway
            .print_project_file("SERIAL1", &command, b"abc".to_vec())
            .await
            .unwrap_err();

        assert!(
            format!("{err:#}").contains("automatic bed leveling"),
            "{}",
            model.unwrap_or("unknown")
        );
        assert!(transfer.recorded_requests().is_empty());
        assert!(mqtt.published_commands().await.is_empty());
    }
}

#[tokio::test]
async fn configured_print_project_file_rejects_nozzle_offset_for_unsupported_models_before_upload()
{
    for model in [
        Some("N7"),
        Some("P1S"),
        Some("A1"),
        Some("Mystery Model"),
        None,
    ] {
        for mode in [1, 2] {
            let mqtt = FakeMqttTransport::default();
            let transfer = FakeMachineFileTransfer::default();
            let mut printer = endpoint("SERIAL1");
            printer.model = model.map(str::to_owned);
            let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
                vec![(printer, mqtt.clone(), transfer.clone())],
                Duration::from_secs(1),
            );
            let mut command = print_project_file();
            command.options.as_mut().unwrap().auto_offset_cali = Some(mode);

            let err = gateway
                .print_project_file("SERIAL1", &command, b"abc".to_vec())
                .await
                .unwrap_err();

            assert!(
                format!("{err:#}").contains("nozzle offset calibration"),
                "{} mode {mode}",
                model.unwrap_or("unknown")
            );
            assert!(transfer.recorded_requests().is_empty());
            assert!(mqtt.published_commands().await.is_empty());
        }
    }
}

#[tokio::test]
async fn configured_print_project_file_accepts_bed_and_nozzle_modes_for_n6_and_h2() {
    for (model, offset_mode) in [("N6", 1), ("H2D", 2)] {
        let mqtt = FakeMqttTransport::default();
        let transfer = FakeMachineFileTransfer::default();
        let mut printer = endpoint("SERIAL1");
        printer.model = Some(model.to_owned());
        let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
            vec![(printer, mqtt.clone(), transfer.clone())],
            Duration::from_secs(1),
        );
        let mut command = print_project_file();
        command.options.as_mut().unwrap().auto_bed_leveling = Some(2);
        command.options.as_mut().unwrap().auto_offset_cali = Some(offset_mode);

        gateway
            .print_project_file("SERIAL1", &command, b"abc".to_vec())
            .await
            .unwrap();

        assert!(!transfer.recorded_requests().is_empty(), "{model}");
        assert!(!mqtt.published_commands().await.is_empty(), "{model}");
    }
}

#[tokio::test]
async fn configured_print_project_file_allows_disabled_calibrations_for_unknown_model() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let mut printer = endpoint("SERIAL1");
    printer.model = None;
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(printer, mqtt.clone(), transfer.clone())],
        Duration::from_secs(1),
    );

    gateway
        .print_project_file("SERIAL1", &print_project_file(), b"abc".to_vec())
        .await
        .unwrap();

    assert!(!transfer.recorded_requests().is_empty());
    assert!(!mqtt.published_commands().await.is_empty());
}

#[tokio::test]
async fn configured_print_project_file_rejects_missing_options_before_upload() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer.clone())],
        Duration::from_secs(1),
    );
    let mut command = print_project_file();
    command.options = None;

    let err = gateway
        .print_project_file("SERIAL1", &command, b"abc".to_vec())
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("missing print project file options"));
    assert!(transfer.recorded_requests().is_empty());
    assert!(mqtt.published_commands().await.is_empty());
}
