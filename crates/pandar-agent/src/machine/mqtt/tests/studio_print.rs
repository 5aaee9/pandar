use super::*;

#[test]
fn project_file_payload_matches_the_typed_studio_print_contract() {
    let payload = BambuMqttCommand::project_file(ProjectFileCommand {
        printer_model: Some("O1C2".to_owned()),
        filename: "gearbox.gcode.3mf".to_owned(),
        url: Some("ftp://gearbox.gcode.3mf".to_owned()),
        md5: Some("900150983CD24FB0D6963F7D28E17F72".to_owned()),
        plate_id: 7,
        studio_submission_id: 38_191,
        submission_source: PrintSubmissionSource::Studio,
        task_name: Some("gearbox task".to_owned()),
        origin_profile_id: 91,
        use_ams: true,
        bed_leveling: false,
        auto_bed_leveling: PrintCalibrationMode::Auto,
        flow_cali: true,
        vibration_cali: true,
        layer_inspect: false,
        auto_flow_cali: PrintCalibrationMode::On,
        auto_offset_cali: PrintCalibrationMode::Auto,
        timelapse: true,
        timelapse_use_internal: true,
        bed_type: "textured_plate".to_owned(),
        extruder_cali_manual_mode: Some(1),
        nozzle_mapping: vec![16, -1, 1],
        ams_mapping: vec![0, 254, -1],
        ams_mapping2: vec![ProjectFileAmsMapping2 {
            ams_id: 255,
            slot_id: 0,
        }],
        ams_mapping_info: vec![ProjectFileAmsMappingInfo {
            ams: 0,
            target_color: "#112233FF".to_owned(),
            filament_id: "GFA00".to_owned(),
            filament_type: "PLA".to_owned(),
            nozzle_id: Some(1),
            source_color: Some("#445566FF".to_owned()),
        }],
    })
    .payload();
    let sequence_id = studio_sequence_id(&payload, "print");

    assert_eq!(
        payload,
        serde_json::json!({
            "print": {
                "command": "project_file",
                "sequence_id": sequence_id,
                "param": "Metadata/plate_7.gcode",
                "project_id": "38191",
                "profile_id": "91",
                "task_id": "38191",
                "subtask_id": "38191",
                "subtask_name": "gearbox task",
                "url": "ftp://gearbox.gcode.3mf",
                "file": "gearbox.gcode.3mf",
                "md5": "900150983CD24FB0D6963F7D28E17F72",
                "bed_type": "textured_plate",
                "bed_leveling": false,
                "flow_cali": true,
                "vibration_cali": true,
                "layer_inspect": false,
                "timelapse": true,
                "use_ams": true,
                "ams_mapping": [0, -1, -1],
                "ams_mapping2": [{"ams_id": 255, "slot_id": 0}],
                "nozzle_mapping": [16, -1, 1],
                "ams_mapping_info": [{
                    "ams": 0,
                    "targetColor": "#112233FF",
                    "filamentId": "GFA00",
                    "filamentType": "PLA",
                    "nozzleId": 1,
                    "sourceColor": "#445566FF"
                }],
                "auto_bed_leveling": 2,
                "nozzle_offset_cali": 2,
                "cfg": "4",
                "extrude_cali_flag": 1,
                "extrude_cali_manual_mode": 1
            }
        })
    );
    assert!(payload["print"].get("nozzles_info").is_none());
}

#[test]
fn web_project_file_payload_preserves_legacy_defaults_without_studio_only_metadata() {
    let payload = BambuMqttCommand::project_file(super::project_file_command()).payload();
    let print = &payload["print"];

    assert_eq!(print["profile_id"], "0");
    assert_eq!(print["bed_type"], "auto");
    assert_eq!(print["vibration_cali"], false);
    assert_eq!(print["layer_inspect"], false);
    assert_eq!(print["cfg"], "0");
    assert!(print.get("extrude_cali_manual_mode").is_none());
    for studio_only_key in [
        "project_name",
        "preset_name",
        "connection_type",
        "comments",
        "submitted_device_name",
        "svc_context",
        "slicer_uid",
    ] {
        assert!(print.get(studio_only_key).is_none(), "{studio_only_key}");
    }
}

#[test]
fn nozzle_mapping_presence_comes_from_the_typed_studio_command() {
    let mut command = super::project_file_command();
    assert!(
        BambuMqttCommand::project_file(command.clone()).payload()["print"]
            .get("nozzle_mapping")
            .is_none()
    );

    command.nozzle_mapping = vec![16, -1];
    assert_eq!(
        BambuMqttCommand::project_file(command).payload()["print"]["nozzle_mapping"],
        serde_json::json!([16, -1])
    );
}
