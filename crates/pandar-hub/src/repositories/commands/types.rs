use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrintProjectFilePayload {
    pub job_id: String,
    pub artifact_id: String,
    pub printer_id: String,
    pub serial_number: String,
    pub filename: String,
    pub storage_path: String,
    #[serde(default)]
    pub artifact_download_path: String,
    pub size_bytes: u64,
    pub plate_id: u32,
    pub use_ams: bool,
    pub flow_cali: bool,
    pub timelapse: bool,
    pub ams_mapping_json: Option<String>,
    pub ams_mapping2_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoverPrintersPayload {
    pub timeout_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosePrinterPayload {
    pub serial_number: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshPrinterMaterialsPayload {
    pub printer_id: String,
    pub serial_number: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkPrinterPayload {
    pub printer_type: String,
    pub host: String,
    pub access_code: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedLinkPrinterPayload {
    pub printer_type: String,
    pub host: String,
    pub access_code: String,
    pub name: Option<String>,
}

impl LinkPrinterPayload {
    pub fn redacted(&self) -> RedactedLinkPrinterPayload {
        RedactedLinkPrinterPayload {
            printer_type: self.printer_type.clone(),
            host: self.host.clone(),
            access_code: "[redacted]".to_owned(),
            name: self.name.clone(),
        }
    }
}
