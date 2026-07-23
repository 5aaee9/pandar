use super::*;

#[test]
fn studio_submission_id_accepts_only_positive_int32_values() {
    assert_eq!(StudioSubmissionId::try_from(1_i64).unwrap().get(), 1);
    assert_eq!(
        StudioSubmissionId::try_from(i64::from(i32::MAX))
            .unwrap()
            .get(),
        i32::MAX
    );
    assert!(StudioSubmissionId::try_from(0_i64).is_err());
    assert!(StudioSubmissionId::try_from(-1_i64).is_err());
    assert!(StudioSubmissionId::try_from(i64::from(i32::MAX) + 1).is_err());
}

#[test]
fn studio_print_metadata_v1_round_trips_closed_typed_json() {
    let metadata = StudioPrintMetadata::V1(StudioPrintMetadataV1 {
        task_name: "gearbox".to_owned(),
        project_name: "motion".to_owned(),
        preset_name: "0.20 Standard".to_owned(),
        config_plate_index: Some(7),
        nozzle_mapping: vec![1, 0],
        ams_mapping: vec![17, 23],
        ams_mapping2: vec![StudioAmsMappingEntry {
            ams_id: 17,
            slot_id: 23,
        }],
        ams_mapping_info: vec![StudioAmsMappingInfo {
            ams: 17,
            target_color: "11223344".to_owned(),
            filament_id: "GFA00".to_owned(),
            filament_type: "PLA".to_owned(),
            nozzle_id: Some(0),
            source_color: Some("55667788".to_owned()),
        }],
        nozzles_info: vec![StudioNozzleInfo {
            id: 0,
            nozzle_type: None,
            flow_size: Some("H".to_owned()),
            diameter: Some(StudioFiniteF64::try_from(0.4).unwrap()),
        }],
        connection_type: "cloud".to_owned(),
        comments: "fixture".to_owned(),
        origin_profile_id: 29,
        stl_design_id: 31,
        origin_model_id: "model-7".to_owned(),
        print_type: "from_normal".to_owned(),
        submitted_device_name: "Workshop X1C".to_owned(),
        task_bed_leveling: true,
        task_flow_cali: true,
        task_vibration_cali: true,
        task_layer_inspect: true,
        task_record_timelapse: true,
        task_timelapse_use_internal: true,
        task_use_ams: true,
        task_bed_type: "pei".to_owned(),
        auto_bed_leveling: PrintCalibrationMode::Auto,
        auto_flow_cali: PrintCalibrationMode::Auto,
        auto_offset_cali: PrintCalibrationMode::Auto,
        extruder_cali_manual_mode: 1,
        try_emmc_print: true,
        svc_context: "service-context".to_owned(),
        slicer_uid: "slicer-uid".to_owned(),
    });

    let encoded = serde_json::to_string(&metadata).unwrap();
    let decoded: StudioPrintMetadata = serde_json::from_str(&encoded).unwrap();

    assert_eq!(decoded, metadata);
    assert!(encoded.contains("\"version\":\"1\""));
    assert!(!encoded.contains("filename"));
    assert!(!encoded.contains("password"));
    assert!(!encoded.contains("dev_ip"));
}

#[test]
fn studio_print_metadata_rejects_unknown_or_non_finite_fields() {
    let unknown = r#"{"version":"1","task_name":"x","project_name":"x","preset_name":"x","nozzle_mapping":[],"ams_mapping":[],"ams_mapping2":[],"ams_mapping_info":[],"nozzles_info":[],"connection_type":"cloud","comments":"","origin_profile_id":0,"stl_design_id":0,"origin_model_id":"","print_type":"from_normal","submitted_device_name":"","task_bed_leveling":false,"task_flow_cali":false,"task_vibration_cali":false,"task_layer_inspect":false,"task_record_timelapse":false,"task_timelapse_use_internal":false,"task_use_ams":false,"task_bed_type":"auto","auto_bed_leveling":0,"auto_flow_cali":0,"auto_offset_cali":0,"extruder_cali_manual_mode":-1,"try_emmc_print":false,"svc_context":"","slicer_uid":"","password":"secret"}"#;
    assert!(serde_json::from_str::<StudioPrintMetadata>(unknown).is_err());
    assert!(StudioFiniteF64::try_from(f64::NAN).is_err());
    assert!(StudioFiniteF64::try_from(f64::INFINITY).is_err());
}

#[test]
fn studio_ams_mapping_info_accepts_pinned_unmatched_entry_shape() {
    let entry: StudioAmsMappingInfo =
        serde_json::from_str(r#"{"ams":-1,"targetColor":"","filamentId":"","filamentType":""}"#)
            .unwrap();

    assert_eq!(entry.nozzle_id, None);
    assert_eq!(entry.source_color, None);
}

#[test]
fn studio_mapping_entries_reject_unknown_fields() {
    assert!(
        serde_json::from_str::<StudioAmsMappingEntry>(
            r#"{"ams_id":1,"slot_id":2,"password":"secret"}"#,
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<StudioAmsMappingInfo>(
            r#"{"ams":1,"targetColor":"11223344","filamentId":"GFA00","filamentType":"PLA","extra":true}"#,
        )
        .is_err()
    );
}
