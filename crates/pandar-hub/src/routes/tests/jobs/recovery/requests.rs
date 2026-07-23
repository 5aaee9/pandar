use crate::material_mapping::{
    AmsMapping, AmsMapping2, AmsMapping2Entry, AmsMappingInfo, AmsMappingInfoEntry,
};
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct RecoveryReasonRequest<'a> {
    reason: Option<&'a str>,
}

#[derive(Serialize)]
struct ReprintJobRequest<'a> {
    reason: Option<&'a str>,
    #[serde(flatten)]
    overrides: DuplicateJobRequest<'a>,
}

#[derive(Serialize)]
struct DuplicateJobRequest<'a> {
    printer_id: &'a str,
    plate_id: i32,
    use_ams: bool,
    bed_leveling: bool,
    auto_bed_leveling: pandar_core::PrintCalibrationMode,
    flow_cali: bool,
    auto_flow_cali: pandar_core::PrintCalibrationMode,
    auto_offset_cali: pandar_core::PrintCalibrationMode,
    timelapse: bool,
    ams_mapping: Option<AmsMapping>,
    ams_mapping2: Option<AmsMapping2>,
    ams_mapping_info: Option<AmsMappingInfo>,
}

#[derive(Serialize)]
struct EmptyRequest {}

pub(super) fn recovery_reason_body(reason: &str) -> Option<Value> {
    Some(
        serde_json::to_value(RecoveryReasonRequest {
            reason: Some(reason),
        })
        .unwrap(),
    )
}

pub(super) fn recovery_reason_null_body() -> Option<Value> {
    Some(serde_json::to_value(RecoveryReasonRequest { reason: None }).unwrap())
}

pub(super) fn duplicate_job_body(printer_id: &str, plate_id: i32) -> Option<Value> {
    Some(
        serde_json::to_value(DuplicateJobRequest {
            printer_id,
            plate_id,
            use_ams: true,
            bed_leveling: false,
            auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
            flow_cali: true,
            auto_flow_cali: pandar_core::PrintCalibrationMode::On,
            auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
            timelapse: false,
            ams_mapping: None,
            ams_mapping2: None,
            ams_mapping_info: None,
        })
        .unwrap(),
    )
}

pub(super) fn reprint_job_body(printer_id: &str, plate_id: i32) -> Option<Value> {
    Some(
        serde_json::to_value(ReprintJobRequest {
            reason: Some("print another"),
            overrides: DuplicateJobRequest {
                printer_id,
                plate_id,
                use_ams: true,
                bed_leveling: false,
                auto_bed_leveling: pandar_core::PrintCalibrationMode::Auto,
                flow_cali: true,
                auto_flow_cali: pandar_core::PrintCalibrationMode::On,
                auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
                timelapse: false,
                ams_mapping: Some(vec![4, 0]),
                ams_mapping2: Some(vec![
                    AmsMapping2Entry {
                        ams_id: 1,
                        slot_id: 0,
                    },
                    AmsMapping2Entry {
                        ams_id: 0,
                        slot_id: 0,
                    },
                ]),
                ams_mapping_info: Some(vec![AmsMappingInfoEntry {
                    ams: 4,
                    target_color: "11223344".to_owned(),
                    filament_id: "GFA00".to_owned(),
                    filament_type: "PLA".to_owned(),
                    nozzle_id: Some(1),
                    source_color: None,
                }]),
            },
        })
        .unwrap(),
    )
}

pub(super) fn empty_body() -> Option<Value> {
    Some(serde_json::to_value(EmptyRequest {}).unwrap())
}
