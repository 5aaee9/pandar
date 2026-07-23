use prost::Message;

use super::super::agent::v1::{
    PrintProjectFile, PrintProjectFileOptions, PrintSubmissionSource, StudioAmsMappingEntry,
    StudioAmsMappingInfo, StudioNozzleInfo, StudioTaskMetadata,
};

#[test]
fn studio_print_wire_round_trips_closed_execution_and_task_metadata() {
    let command = PrintProjectFile {
        job_id: "job-uuid".into(),
        artifact_id: "artifact-uuid".into(),
        printer_id: "printer-uuid".into(),
        serial_number: "01P00A000000001".into(),
        filename: "gearbox.3mf".into(),
        storage_path: "tenant/artifacts/gearbox.3mf".into(),
        size_bytes: 123,
        plate_id: 7,
        artifact_download_path: "/api/v1/agents/a/artifacts/b".into(),
        studio_submission_id: 38_191,
        options: Some(PrintProjectFileOptions {
            use_ams: true,
            bed_leveling: false,
            flow_cali: true,
            vibration_cali: true,
            layer_inspect: false,
            record_timelapse: true,
            timelapse_use_internal: true,
            bed_type: "textured_plate".into(),
            auto_bed_leveling: Some(2),
            auto_flow_cali: Some(1),
            auto_offset_cali: Some(0),
            extruder_cali_manual_mode: Some(-1),
            try_emmc_print: true,
            nozzle_mapping: vec![16, -1, 1],
            ams_mapping: vec![0, 254, -1],
            ams_mapping2: vec![StudioAmsMappingEntry {
                ams_id: 255,
                slot_id: 0,
            }],
            ams_mapping_info: vec![StudioAmsMappingInfo {
                ams: 0,
                target_color: "#112233FF".into(),
                filament_id: "GFA00".into(),
                filament_type: "PLA".into(),
                nozzle_id: Some(1),
                source_color: Some("#445566FF".into()),
            }],
            nozzles_info: vec![StudioNozzleInfo {
                id: 1,
                nozzle_type: Some("hardened_steel".into()),
                flow_size: Some("standard".into()),
                diameter: Some(0.4),
            }],
        }),
        task_metadata: Some(StudioTaskMetadata {
            task_name: "gearbox task".into(),
            project_name: "gearbox project".into(),
            preset_name: "0.20 Standard".into(),
            connection_type: "cloud".into(),
            comments: "no_ip".into(),
            origin_profile_id: 91,
            stl_design_id: 92,
            origin_model_id: "model-93".into(),
            print_type: "from_normal".into(),
            submitted_device_name: "Workshop X1C".into(),
            svc_context: "svc".into(),
            slicer_uid: "slicer".into(),
        }),
        submission_source: PrintSubmissionSource::Studio as i32,
    };

    let encoded = command.encode_to_vec();
    let decoded = PrintProjectFile::decode(encoded.as_slice()).unwrap();

    assert_eq!(decoded, command);
    assert_eq!(decoded.studio_submission_id, 38_191);
    assert_eq!(decoded.options.unwrap().extruder_cali_manual_mode, Some(-1));
}
