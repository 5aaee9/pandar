use pandar_core::{
    H2cAutoMappingFilamentInfo, H2cAutoMappingGroupInfo, H2cAutoNozzleMappingRequest,
    H2cAutoNozzleMappingResponseEnvelope,
};
use serde_json::json;

fn v1_request() -> H2cAutoNozzleMappingRequest {
    H2cAutoNozzleMappingRequest {
        command: "get_auto_nozzle_mapping".to_owned(),
        sequence_id: "42".to_owned(),
        version: Some(1),
        calibration: None,
        extrude_cali_manual_mode: None,
        filament_seq: None,
        ams_mapping: None,
        fila_info: None,
        nozzle_info: None,
        group_info: Some(vec![H2cAutoMappingGroupInfo {
            id: 0,
            ext: 1,
            dia: 0.4,
            vol: "E3D High Flow".to_owned(),
        }]),
    }
}

fn response(value: serde_json::Value) -> H2cAutoNozzleMappingResponseEnvelope {
    serde_json::from_value(value).unwrap()
}

#[test]
fn v1_request_accepts_e3d_high_flow() {
    assert!(v1_request().is_valid());
}

#[test]
fn v0_request_allows_filament_nozzle_combinations_beyond_ams_slot_count() {
    let filament = H2cAutoMappingFilamentInfo {
        id: 1,
        direction: 1,
        group: 0,
        nozzle_d: "0.4".to_owned(),
        nozzle_v: "Standard".to_owned(),
        cate: "GFA00".to_owned(),
        color: "FFFFFFFF".to_owned(),
    };
    let request = H2cAutoNozzleMappingRequest {
        command: "get_auto_nozzle_mapping".to_owned(),
        sequence_id: "41".to_owned(),
        version: None,
        calibration: Some(0),
        extrude_cali_manual_mode: Some(0),
        filament_seq: Some(vec![0]),
        ams_mapping: Some(vec![0xffff; 33]),
        fila_info: Some(vec![filament; 34]),
        nozzle_info: Some(Vec::new()),
        group_info: None,
    };

    assert!(request.is_valid());
}

#[test]
fn correlated_failure_preserves_detail_without_a_valid_version() {
    let request = v1_request();
    let response = response(json!({
        "print": {
            "command": "get_auto_nozzle_mapping",
            "sequence_id": "42",
            "result": "fail",
            "version": "future",
            "reason": "rack busy",
            "errno": 17
        }
    }));

    assert!(response.is_valid_for(&request));
    assert_eq!(response.print.reason.as_deref(), Some("rack busy"));
    assert_eq!(response.print.errno, Some(17));
}

#[test]
fn success_requires_matching_version_and_physical_nozzle_ids() {
    let request = v1_request();
    let valid = response(json!({
        "print": {
            "command": "get_auto_nozzle_mapping",
            "sequence_id": "42",
            "result": "success",
            "version": 1,
            "mapping": [16, 21, -1]
        }
    }));
    assert!(valid.is_valid_for(&request));

    for invalid in [
        json!({
            "print": {
                "command": "get_auto_nozzle_mapping",
                "sequence_id": "42",
                "result": "success",
                "version": 0,
                "mapping": [16]
            }
        }),
        json!({
            "print": {
                "command": "get_auto_nozzle_mapping",
                "sequence_id": "42",
                "result": "success",
                "version": "1",
                "mapping": [16]
            }
        }),
        json!({
            "print": {
                "command": "get_auto_nozzle_mapping",
                "sequence_id": "42",
                "result": "success",
                "version": 1,
                "mapping": [22]
            }
        }),
        json!({
            "print": {
                "command": "other",
                "sequence_id": "42",
                "result": "success",
                "version": 1,
                "mapping": [16]
            }
        }),
        json!({
            "print": {
                "command": "get_auto_nozzle_mapping",
                "sequence_id": "43",
                "result": "success",
                "version": 1,
                "mapping": [16]
            }
        }),
    ] {
        assert!(!response(invalid).is_valid_for(&request));
    }
}
