use pandar_core::PrinterFirmwareModule;
use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct GetVersionCommandEnvelope {
    #[serde(default)]
    pub(super) info: Option<GetVersionCommand>,
}

#[derive(Deserialize)]
pub(super) struct GetVersionCommand {
    #[serde(default)]
    pub(super) command: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct GetVersionReport {
    pub(super) info: GetVersionInfo,
}

#[derive(Deserialize)]
pub(super) struct GetVersionInfo {
    #[serde(default)]
    pub(super) module: Option<Vec<PrinterFirmwareModule>>,
}

#[derive(Deserialize)]
pub(super) struct FirmwareAcknowledgementEnvelope {
    #[serde(default)]
    pub(super) upgrade: Option<FirmwareAcknowledgementFields>,
}

#[derive(Deserialize)]
pub(super) struct FirmwareAcknowledgementFields {
    #[serde(default)]
    pub(super) command: Option<String>,
    #[serde(default)]
    pub(super) sequence_id: Option<String>,
    #[serde(default)]
    pub(super) result: Option<String>,
    #[serde(default, rename = "err_code")]
    pub(super) error_code: Option<i64>,
    #[serde(default)]
    pub(super) reason: Option<String>,
    #[serde(default)]
    pub(super) message: Option<String>,
}
