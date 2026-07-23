use super::*;

#[test]
fn project_file_payload_reserves_dispatch_identity_and_flags() {
    let payload = BambuMqttCommand::project_file(ProjectFileCommand {
        bed_leveling: true,
        auto_bed_leveling: PrintCalibrationMode::Auto,
        flow_cali: true,
        vibration_cali: true,
        layer_inspect: true,
        auto_flow_cali: PrintCalibrationMode::Auto,
        auto_offset_cali: PrintCalibrationMode::On,
        timelapse_use_internal: true,
        bed_type: "textured_plate".to_owned(),
        submission_source: PrintSubmissionSource::Studio,
        extruder_cali_manual_mode: Some(1),
        ..project_file_command()
    })
    .payload();

    let sequence_id = studio_sequence_id(&payload, "print");
    let print = project_file_payload(&payload).print;
    assert_eq!(print.command, "project_file");
    assert_eq!(print.sequence_id, sequence_id);
    assert_eq!(print.param, "Metadata/plate_2.gcode");
    assert_eq!(print.profile_id, "38191");
    assert_eq!(print.subtask_name, "job");
    assert_eq!(print.url, "ftp://job.3mf");
    assert_eq!(print.file, "job.3mf");
    assert_eq!(print.md5, "");
    assert_eq!(print.bed_type, "textured_plate");
    assert!(print.bed_leveling);
    assert!(print.flow_cali);
    assert!(print.vibration_cali);
    assert!(print.layer_inspect);
    assert!(!print.timelapse);
    assert!(print.use_ams);
    assert_eq!(print.ams_mapping, Vec::<i64>::new());
    assert_eq!(print.ams_mapping2, Vec::<TestAmsMapping2>::new());
    assert_eq!(print.nozzle_mapping, None);
    assert_eq!(print.ams_mapping_info, None);
    assert_eq!(print.auto_bed_leveling, 2);
    assert_eq!(print.nozzle_offset_cali, 1);
    assert_eq!(print.cfg, "4");
    assert_eq!(print.extrude_cali_flag, 2);
    assert_eq!(print.extrude_cali_manual_mode, Some(1));

    let project_id = print.project_id.parse::<u32>().unwrap();
    assert_eq!(project_id, 38_191);
    assert_eq!(print.task_id, print.project_id);
    assert_eq!(print.subtask_id, print.project_id);
}

#[test]
fn project_file_payload_matches_studio_auto_calibration_combination() {
    let payload = BambuMqttCommand::project_file(ProjectFileCommand {
        printer_model: Some("N6".to_owned()),
        plate_id: 1,
        auto_bed_leveling: PrintCalibrationMode::Auto,
        auto_flow_cali: PrintCalibrationMode::Auto,
        timelapse: true,
        ..project_file_command()
    })
    .payload();

    let print = project_file_payload(&payload).print;
    assert!(!print.bed_leveling);
    assert_eq!(print.auto_bed_leveling, 2);
    assert!(!print.flow_cali);
    assert_eq!(print.extrude_cali_flag, 2);
    assert_eq!(print.nozzle_offset_cali, 0);
    assert!(print.timelapse);
}
#[test]
fn project_file_payload_defaults_mapping_keys_when_no_mapping_supplied() {
    let payload = BambuMqttCommand::project_file(ProjectFileCommand {
        use_ams: false,
        ..project_file_command()
    })
    .payload();

    let print = project_file_payload(&payload).print;
    assert_eq!(print.ams_mapping, Vec::<i64>::new());
    assert_eq!(print.ams_mapping2, Vec::<TestAmsMapping2>::new());
    assert!(!print.use_ams);
}

#[test]
fn project_file_payload_includes_ams_mapping_only_when_supplied() {
    let payload = BambuMqttCommand::project_file(ProjectFileCommand {
        ams_mapping: vec![0, -1, 4],
        ..project_file_command()
    })
    .payload();

    let print = project_file_payload(&payload).print;
    assert_eq!(print.ams_mapping, vec![0, -1, 4]);
    assert_eq!(print.ams_mapping2, Vec::<TestAmsMapping2>::new());
    assert!(print.use_ams);
}

#[test]
fn project_file_payload_includes_ams_mapping2_only_when_supplied() {
    let payload = BambuMqttCommand::project_file(ProjectFileCommand {
        ams_mapping2: vec![ProjectFileAmsMapping2 {
            ams_id: 255,
            slot_id: 0,
        }],
        ..project_file_command()
    })
    .payload();

    let print = project_file_payload(&payload).print;
    assert_eq!(print.ams_mapping, Vec::<i64>::new());
    assert_eq!(
        print.ams_mapping2,
        vec![TestAmsMapping2 {
            ams_id: 255,
            slot_id: 0
        }]
    );
}

#[test]
fn project_file_payload_includes_both_mapping_keys_when_supplied() {
    let payload = BambuMqttCommand::project_file(ProjectFileCommand {
        ams_mapping: vec![0, 1],
        ams_mapping2: vec![ProjectFileAmsMapping2 {
            ams_id: 0,
            slot_id: 1,
        }],
        ams_mapping_info: vec![ProjectFileAmsMappingInfo {
            ams: 0,
            target_color: "#112233FF".to_owned(),
            filament_id: "GFA00".to_owned(),
            filament_type: "PLA".to_owned(),
            nozzle_id: Some(0),
            source_color: None,
        }],
        ..project_file_command()
    })
    .payload();

    let print = project_file_payload(&payload).print;
    assert_eq!(print.ams_mapping, vec![0, 1]);
    assert_eq!(
        print.ams_mapping2,
        vec![TestAmsMapping2 {
            ams_id: 0,
            slot_id: 1
        }]
    );
    assert_eq!(
        print.ams_mapping_info,
        Some(vec![TestAmsMappingInfo {
            ams: 0,
            target_color: "#112233FF".to_owned(),
            filament_id: "GFA00".to_owned(),
            filament_type: "PLA".to_owned(),
            nozzle_id: Some(0),
            source_color: None,
        }])
    );
}

#[test]
fn project_file_payload_rewrites_flat_external_mapping_values() {
    let payload = BambuMqttCommand::project_file(ProjectFileCommand {
        ams_mapping: vec![254, 255, 15],
        ..project_file_command()
    })
    .payload();

    assert_eq!(
        project_file_payload(&payload).print.ams_mapping,
        vec![-1, -1, 15]
    );
}
