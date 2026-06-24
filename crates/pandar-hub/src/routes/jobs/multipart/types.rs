use serde_json::Value;

#[derive(Debug, Default)]
pub(in crate::routes::jobs) struct MultipartPrintFields {
    pub(super) printer_id: Option<String>,
    pub(in crate::routes::jobs) filename: Option<String>,
    pub(in crate::routes::jobs) content_type: Option<String>,
    pub(super) plate_id: Option<i64>,
    pub(super) use_ams: Option<bool>,
    pub(super) flow_cali: Option<bool>,
    pub(super) timelapse: Option<bool>,
    pub(super) ams_mapping: Option<Value>,
    pub(super) ams_mapping2: Option<Value>,
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
