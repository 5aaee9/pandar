use crate::artifacts::metadata::ArtifactMetadata;
use crate::material_mapping::{AmsMapping, AmsMapping2, AmsMappingInfo};
use pandar_core::{PrintCalibrationMode, Printer};
use tokio::fs;

#[derive(Debug, Default)]
pub(in crate::routes::jobs) struct MultipartPrintFields {
    pub(super) printer_id: Option<String>,
    pub(in crate::routes::jobs) filename: Option<String>,
    pub(in crate::routes::jobs) content_type: Option<String>,
    pub(super) plate_id: Option<i64>,
    pub(super) use_ams: Option<bool>,
    pub(super) bed_leveling: Option<bool>,
    pub(super) auto_bed_leveling: Option<PrintCalibrationMode>,
    pub(super) flow_cali: Option<bool>,
    pub(super) auto_flow_cali: Option<PrintCalibrationMode>,
    pub(super) auto_offset_cali: Option<PrintCalibrationMode>,
    pub(super) timelapse: Option<bool>,
    pub(super) ams_mapping: Option<AmsMapping>,
    pub(super) ams_mapping2: Option<AmsMapping2>,
    pub(super) ams_mapping_info: Option<AmsMappingInfo>,
    pub(in crate::routes::jobs) file: Option<StagedUpload>,
}

impl MultipartPrintFields {
    pub(in crate::routes::jobs) async fn cleanup_staged_uploads(&self) {
        if let Some(file) = &self.file {
            super::cleanup_staged_upload(file).await;
        }
    }
}

#[derive(Debug)]
pub(in crate::routes::jobs) struct StagedUpload {
    pub(in crate::routes::jobs) path: std::path::PathBuf,
    pub(in crate::routes::jobs) filename: Option<String>,
    pub(in crate::routes::jobs) content_type: Option<String>,
}

pub(super) struct PreparedPrintJob {
    pub(super) printer: Printer,
    pub(super) plate_id: u32,
    pub(super) ams_mapping_json: Option<String>,
    pub(super) ams_mapping2_json: Option<String>,
    pub(super) ams_mapping_info_json: Option<String>,
    pub(super) use_ams: bool,
    pub(super) bed_leveling: bool,
    pub(super) auto_bed_leveling: PrintCalibrationMode,
    pub(super) flow_cali: bool,
    pub(super) auto_flow_cali: PrintCalibrationMode,
    pub(super) auto_offset_cali: PrintCalibrationMode,
    pub(super) timelapse: bool,
    pub(super) filename: String,
    pub(super) content_type: String,
    pub(super) artifact_metadata: Option<ArtifactMetadata>,
    pub(super) upload_file: fs::File,
}
