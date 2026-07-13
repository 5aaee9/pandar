use pandar_core::{
    FirmwareCatalogEntry, FirmwareTerminalOutcome, PrinterFirmwareModule, PrinterFirmwareState,
    PrinterFirmwareStatus,
};
use serde::{Deserialize, Serialize};

use crate::firmware::StudioFirmwareCommand;

#[derive(Deserialize)]
pub(super) struct PreparedResponse {
    #[serde(rename = "command_id")]
    _command_id: String,
    pub(super) prepared_token: String,
}

#[derive(Deserialize)]
pub(super) struct ExecuteResponse {
    #[serde(rename = "command_id")]
    _command_id: String,
    pub(super) phase: ExecutePhase,
    pub(super) outcome: Option<FirmwareTerminalOutcome>,
    pub(super) transient_status: Option<PrinterFirmwareStatus>,
    #[serde(rename = "error")]
    _error: Option<String>,
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum ExecutePhase {
    PrePublishFailure,
    Acknowledged,
    Rejected,
    OutcomeUnknown,
}

#[derive(Deserialize)]
pub(super) struct ErrorResponse {
    pub(super) phase: Option<ExecutePhase>,
    #[serde(rename = "error")]
    _error: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct FirmwareStateResponse {
    #[serde(rename = "firmware")]
    _firmware: PrinterFirmwareState,
    pub(super) catalog: Vec<FirmwareCatalogEntry>,
}

#[derive(Serialize)]
pub(super) struct RefreshRequest<'a> {
    pub(super) sequence_id: &'a str,
}

#[derive(Deserialize)]
pub(super) struct RefreshResponse {
    #[serde(rename = "command_id")]
    _command_id: String,
    pub(super) modules: Vec<PrinterFirmwareModule>,
    #[serde(rename = "module_revision")]
    _module_revision: u64,
}

#[derive(Serialize)]
pub(super) struct ExecuteRequest<'a> {
    pub(super) prepared_token: &'a str,
    pub(super) command: ExecuteCommand<'a>,
}

#[derive(Serialize)]
#[serde(tag = "command")]
pub(super) enum ExecuteCommand<'a> {
    #[serde(rename = "upgrade_confirm")]
    UpgradeConfirm { sequence_id: &'a str, src_id: i64 },
    #[serde(rename = "consistency_confirm")]
    ConsistencyConfirm { sequence_id: &'a str, src_id: i64 },
    #[serde(rename = "start")]
    Start {
        sequence_id: &'a str,
        src_id: i64,
        url: &'a str,
        module: &'a str,
        version: &'a str,
    },
    #[serde(rename = "mc_for_ams_firmware_upgrade")]
    SwitchAmsFirmware {
        sequence_id: &'a str,
        src_id: i64,
        id: i32,
    },
}

impl<'a> From<&'a StudioFirmwareCommand> for ExecuteCommand<'a> {
    fn from(command: &'a StudioFirmwareCommand) -> Self {
        match command {
            StudioFirmwareCommand::UpgradeConfirm {
                sequence_id,
                src_id,
            } => Self::UpgradeConfirm {
                sequence_id,
                src_id: *src_id,
            },
            StudioFirmwareCommand::ConsistencyConfirm {
                sequence_id,
                src_id,
            } => Self::ConsistencyConfirm {
                sequence_id,
                src_id: *src_id,
            },
            StudioFirmwareCommand::Start {
                sequence_id,
                src_id,
                url,
                module,
                version,
            } => Self::Start {
                sequence_id,
                src_id: *src_id,
                url,
                module,
                version,
            },
            StudioFirmwareCommand::McForAmsFirmwareUpgrade {
                sequence_id,
                src_id,
                id,
            } => Self::SwitchAmsFirmware {
                sequence_id,
                src_id: *src_id,
                id: *id,
            },
        }
    }
}
