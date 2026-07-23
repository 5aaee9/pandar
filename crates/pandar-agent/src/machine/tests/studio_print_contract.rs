use std::time::Duration;

use super::{ConfiguredBambuMachineGateway, TransferModeCache, endpoint, print_project_file};
use crate::machine::{
    BambuMachineGateway,
    file_transfer::{FakeMachineFileTransfer, FileTransferOperation},
    mqtt::{BAMBU_MQTT_QOS, FakeMqttTransport},
};
use crate::protocol::agent::v1::{PrintProjectFile, PrintSubmissionSource, StudioNozzleInfo};

#[tokio::test]
async fn print_transfer_policy_is_forwarded_from_each_command() {
    for try_emmc_print in [false, true] {
        let mqtt = FakeMqttTransport::default();
        let transfer = FakeMachineFileTransfer::default();
        let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
            vec![(endpoint("SERIAL1"), mqtt.clone(), transfer.clone())],
            Duration::from_secs(1),
            TransferModeCache::default(),
        );
        let mut command = print_project_file();
        command.options.as_mut().unwrap().try_emmc_print = try_emmc_print;

        gateway
            .print_project_file("SERIAL1", &command, b"abc".to_vec())
            .await
            .unwrap();

        let recorded = transfer.recorded_requests();
        assert_eq!(
            recorded[0].1.operation,
            FileTransferOperation::PrintUpload {
                size_bytes: 3,
                try_emmc_print,
            }
        );
        assert_eq!(mqtt.published_commands().await[0].qos, BAMBU_MQTT_QOS);
    }
}

#[tokio::test]
async fn corrupt_typed_print_commands_fail_before_upload_and_mqtt() {
    let mut cases = Vec::new();

    let mut unspecified_source = print_project_file();
    unspecified_source.submission_source = PrintSubmissionSource::Unspecified as i32;
    cases.push((unspecified_source, "submission source"));

    let mut zero_id = print_project_file();
    zero_id.studio_submission_id = 0;
    cases.push((zero_id, "studio_submission_id"));

    let mut zero_plate = print_project_file();
    zero_plate.plate_id = 0;
    cases.push((zero_plate, "plate_id"));

    let mut overflowing_plate = print_project_file();
    overflowing_plate.plate_id = i32::MAX as u32 + 1;
    cases.push((overflowing_plate, "plate_id"));

    let mut overflowing_id = print_project_file();
    overflowing_id.studio_submission_id = i32::MAX as u32 + 1;
    cases.push((overflowing_id, "studio_submission_id"));

    let mut missing_metadata = print_project_file();
    missing_metadata.task_metadata = None;
    cases.push((missing_metadata, "task metadata"));

    let mut web_with_studio_metadata = print_project_file();
    web_with_studio_metadata.submission_source = PrintSubmissionSource::Web as i32;
    cases.push((
        web_with_studio_metadata,
        "must not contain Studio task metadata",
    ));

    let mut invalid_enum = print_project_file();
    invalid_enum.options.as_mut().unwrap().auto_flow_cali = Some(3);
    cases.push((invalid_enum, "auto_flow_cali"));

    let mut missing_enum = print_project_file();
    missing_enum.options.as_mut().unwrap().auto_bed_leveling = None;
    cases.push((missing_enum, "missing auto_bed_leveling"));

    let mut web_missing_enum = print_project_file();
    web_missing_enum.submission_source = PrintSubmissionSource::Web as i32;
    web_missing_enum.task_metadata = None;
    let web_options = web_missing_enum.options.as_mut().unwrap();
    web_options.extruder_cali_manual_mode = None;
    web_options.auto_flow_cali = None;
    cases.push((web_missing_enum, "missing auto_flow_cali"));

    let mut invalid_manual_mode = print_project_file();
    invalid_manual_mode
        .options
        .as_mut()
        .unwrap()
        .extruder_cali_manual_mode = Some(2);
    cases.push((invalid_manual_mode, "extruder_cali_manual_mode"));

    let mut missing_manual_mode = print_project_file();
    missing_manual_mode
        .options
        .as_mut()
        .unwrap()
        .extruder_cali_manual_mode = None;
    cases.push((missing_manual_mode, "missing extruder_cali_manual_mode"));

    let mut non_finite_nozzle = print_project_file();
    non_finite_nozzle
        .options
        .as_mut()
        .unwrap()
        .nozzles_info
        .push(StudioNozzleInfo {
            id: 0,
            nozzle_type: None,
            flow_size: None,
            diameter: Some(f64::NAN),
        });
    cases.push((non_finite_nozzle, "nozzles_info diameter"));

    let mut oversized_mapping = print_project_file();
    oversized_mapping.options.as_mut().unwrap().nozzle_mapping = vec![0; 33];
    cases.push((oversized_mapping, "nozzle_mapping"));

    for (command, expected_error) in cases {
        assert_rejected_before_side_effect(command, expected_error).await;
    }
}

#[tokio::test]
async fn studio_print_rejects_non_concrete_bed_types_before_side_effects() {
    for bed_type in ["auto", "unknown", "custom_plate"] {
        let mut command = print_project_file();
        command.options.as_mut().unwrap().bed_type = bed_type.to_owned();

        assert_rejected_before_side_effect(command, "invalid Studio bed_type").await;
    }
}

#[tokio::test]
async fn studio_print_accepts_each_pinned_concrete_bed_type() {
    for bed_type in [
        "supertack_plate",
        "cool_plate",
        "eng_plate",
        "hot_plate",
        "textured_plate",
    ] {
        let mqtt = FakeMqttTransport::default();
        let transfer = FakeMachineFileTransfer::default();
        let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
            vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
            Duration::from_secs(1),
            TransferModeCache::default(),
        );
        let mut command = print_project_file();
        command.options.as_mut().unwrap().bed_type = bed_type.to_owned();

        gateway
            .print_project_file("SERIAL1", &command, b"abc".to_vec())
            .await
            .unwrap();

        assert_eq!(
            mqtt.published_commands().await[0].payload["print"]["bed_type"],
            bed_type
        );
    }
}

#[tokio::test]
async fn web_print_uses_only_web_fields_and_omits_studio_manual_mode() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer.clone())],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );
    let mut command = print_project_file();
    command.submission_source = PrintSubmissionSource::Web as i32;
    command.task_metadata = None;
    let options = command.options.as_mut().unwrap();
    options.bed_type.clear();
    options.extruder_cali_manual_mode = None;
    options.try_emmc_print = true;

    gateway
        .print_project_file("SERIAL1", &command, b"abc".to_vec())
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let print = &published[0].payload["print"];
    assert_eq!(print["profile_id"], "0");
    assert_eq!(print["bed_type"], "auto");
    assert_eq!(print["auto_bed_leveling"], 0);
    assert_eq!(print["extrude_cali_flag"], 0);
    assert!(print.get("extrude_cali_manual_mode").is_none());
    assert_eq!(
        transfer.recorded_requests()[0].1.operation,
        FileTransferOperation::PrintUpload {
            size_bytes: 3,
            try_emmc_print: true,
        }
    );
}

async fn assert_rejected_before_side_effect(command: PrintProjectFile, expected_error: &str) {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer.clone())],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    let error = gateway
        .print_project_file("SERIAL1", &command, b"abc".to_vec())
        .await
        .unwrap_err();

    assert!(format!("{error:#}").contains(expected_error), "{error:#}");
    assert!(transfer.recorded_requests().is_empty());
    assert!(mqtt.published_commands().await.is_empty());
}
