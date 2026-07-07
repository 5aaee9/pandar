use crate::artifacts::metadata::ArtifactMetadata;
use crate::material_mapping::{AmsMapping, AmsMapping2, AmsMappingInfo};
use pandar_core::Printer;
use tokio::fs;

#[derive(Debug, Default)]
pub(in crate::routes::jobs) struct MultipartPrintFields {
    pub(super) printer_id: Option<String>,
    pub(in crate::routes::jobs) filename: Option<String>,
    pub(in crate::routes::jobs) content_type: Option<String>,
    pub(super) plate_id: Option<i64>,
    pub(super) use_ams: Option<bool>,
    pub(super) flow_cali: Option<bool>,
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

pub(super) type PreparedPrintJob = (
    Printer,
    u32,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
    bool,
    bool,
    String,
    String,
    Option<ArtifactMetadata>,
    fs::File,
);
