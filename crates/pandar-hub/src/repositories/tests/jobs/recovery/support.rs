use super::*;

pub(super) fn with_studio_auto_calibration(mut input: CreatePrintJob) -> CreatePrintJob {
    input.bed_leveling = true;
    input.auto_bed_leveling = PrintCalibrationMode::Auto;
    input.flow_cali = false;
    input.auto_flow_cali = PrintCalibrationMode::Auto;
    input.auto_offset_cali = PrintCalibrationMode::On;
    input.timelapse = true;
    input
}

pub(super) fn assert_studio_auto_calibration(payload: &PrintProjectFilePayload) {
    assert!(payload.bed_leveling);
    assert_eq!(payload.auto_bed_leveling, PrintCalibrationMode::Auto);
    assert!(!payload.flow_cali);
    assert_eq!(payload.auto_flow_cali, PrintCalibrationMode::Auto);
    assert_eq!(payload.auto_offset_cali, PrintCalibrationMode::On);
    assert!(payload.timelapse);
}
