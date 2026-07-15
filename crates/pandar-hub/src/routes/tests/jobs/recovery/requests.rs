use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct RecoveryReasonRequest<'a> {
    reason: Option<&'a str>,
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
    ams_mapping: Option<()>,
    ams_mapping2: Option<()>,
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
        })
        .unwrap(),
    )
}

pub(super) fn empty_body() -> Option<Value> {
    Some(serde_json::to_value(EmptyRequest {}).unwrap())
}
