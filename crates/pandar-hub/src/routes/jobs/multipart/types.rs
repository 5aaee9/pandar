use crate::artifacts::metadata::ArtifactMetadata;
use crate::material_mapping::{AmsMapping, AmsMapping2, AmsMappingInfo};
use pandar_core::{PrintCalibrationMode, Printer, StudioNozzleInfo};
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
    pub(super) task_name: Option<String>,
    pub(super) project_name: Option<String>,
    pub(super) preset_name: Option<String>,
    pub(super) config_plate_index: Option<u32>,
    pub(super) nozzle_mapping: Option<Vec<i32>>,
    pub(super) nozzles_info: Option<Vec<StudioNozzleInfo>>,
    pub(super) connection_type: Option<String>,
    pub(super) comments: Option<String>,
    pub(super) origin_profile_id: Option<i64>,
    pub(super) stl_design_id: Option<i64>,
    pub(super) origin_model_id: Option<String>,
    pub(super) print_type: Option<String>,
    pub(super) submitted_device_name: Option<String>,
    pub(super) vibration_cali: Option<bool>,
    pub(super) layer_inspect: Option<bool>,
    pub(super) timelapse_use_internal: Option<bool>,
    pub(super) bed_type: Option<String>,
    pub(super) extruder_cali_manual_mode: Option<i8>,
    pub(super) try_emmc_print: Option<bool>,
    pub(super) svc_context: Option<String>,
    pub(super) slicer_uid: Option<String>,
    pub(super) unknown_field: bool,
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
