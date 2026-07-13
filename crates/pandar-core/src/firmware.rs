use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrinterFirmwareModule {
    pub name: String,
    #[serde(rename = "sw_ver", skip_serializing_if = "Option::is_none")]
    pub software_version: Option<String>,
    #[serde(rename = "sw_new_ver", skip_serializing_if = "Option::is_none")]
    pub software_new_version: Option<String>,
    #[serde(rename = "new_ver", skip_serializing_if = "Option::is_none")]
    pub new_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_name: Option<String>,
    #[serde(rename = "sn", skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    #[serde(rename = "hw_ver", skip_serializing_if = "Option::is_none")]
    pub hardware_version: Option<String>,
    #[serde(rename = "flag", skip_serializing_if = "Option::is_none")]
    pub firmware_flag: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrinterFirmwareVersion {
    pub name: String,
    #[serde(rename = "cur_ver", skip_serializing_if = "Option::is_none")]
    pub current_version: Option<String>,
    #[serde(rename = "new_ver", skip_serializing_if = "Option::is_none")]
    pub new_version: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AmsFirmwareDescriptor {
    pub id: i32,
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AmsFirmwareSwitchState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware: Option<Vec<AmsFirmwareDescriptor>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_firmware_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_run_firmware_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrinterUpgradeState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    #[serde(rename = "err_code", skip_serializing_if = "Option::is_none")]
    pub error_code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_version_state: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consistency_request: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_upgrade: Option<bool>,
    #[serde(rename = "dis_state", skip_serializing_if = "Option::is_none")]
    pub display_state: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ota_new_version_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ams_new_version_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ahb_new_version_number: Option<String>,
    #[serde(rename = "new_ver_list", skip_serializing_if = "Option::is_none")]
    pub new_versions: Option<Vec<PrinterFirmwareVersion>>,
    #[serde(
        rename = "mc_for_ams_firmware",
        skip_serializing_if = "Option::is_none"
    )]
    pub ams_firmware: Option<AmsFirmwareSwitchState>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrinterFirmwareStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upgrade_state: Option<PrinterUpgradeState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cfg: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrinterFirmwareState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
    pub module_revision: u64,
    pub status_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modules: Option<Vec<PrinterFirmwareModule>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upgrade_state: Option<PrinterUpgradeState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cfg: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FirmwareCatalogTarget {
    Printer,
    Ams,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FirmwareCatalogEntry {
    pub target: FirmwareCatalogTarget,
    pub version: String,
    pub url: String,
    pub description: String,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
#[serde(tag = "command")]
pub enum FirmwareCommand {
    #[serde(rename = "upgrade_confirm")]
    UpgradeConfirm { sequence_id: String, src_id: i64 },
    #[serde(rename = "consistency_confirm")]
    ConsistencyConfirm { sequence_id: String, src_id: i64 },
    #[serde(rename = "start")]
    Start {
        sequence_id: String,
        src_id: i64,
        url: String,
        module: String,
        version: String,
    },
    #[serde(rename = "mc_for_ams_firmware_upgrade")]
    SwitchAmsFirmware {
        sequence_id: String,
        src_id: i64,
        id: i32,
    },
}

impl fmt::Debug for FirmwareCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UpgradeConfirm {
                sequence_id,
                src_id,
            } => formatter
                .debug_struct("UpgradeConfirm")
                .field("sequence_id", sequence_id)
                .field("src_id", src_id)
                .finish(),
            Self::ConsistencyConfirm {
                sequence_id,
                src_id,
            } => formatter
                .debug_struct("ConsistencyConfirm")
                .field("sequence_id", sequence_id)
                .field("src_id", src_id)
                .finish(),
            Self::Start {
                sequence_id,
                src_id,
                module,
                version,
                ..
            } => formatter
                .debug_struct("Start")
                .field("sequence_id", sequence_id)
                .field("src_id", src_id)
                .field("url", &"[redacted]")
                .field("module", module)
                .field("version", version)
                .finish(),
            Self::SwitchAmsFirmware {
                sequence_id,
                src_id,
                id,
            } => formatter
                .debug_struct("SwitchAmsFirmware")
                .field("sequence_id", sequence_id)
                .field("src_id", src_id)
                .field("id", id)
                .finish(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "command")]
pub enum FirmwareControlMetadata {
    #[serde(rename = "upgrade_confirm")]
    UpgradeConfirm { sequence_id: String, src_id: i64 },
    #[serde(rename = "consistency_confirm")]
    ConsistencyConfirm { sequence_id: String, src_id: i64 },
    #[serde(rename = "start")]
    Start {
        sequence_id: String,
        src_id: i64,
        module: String,
        version: String,
    },
    #[serde(rename = "mc_for_ams_firmware_upgrade")]
    SwitchAmsFirmware {
        sequence_id: String,
        src_id: i64,
        id: i32,
    },
}

impl From<&FirmwareCommand> for FirmwareControlMetadata {
    fn from(command: &FirmwareCommand) -> Self {
        match command {
            FirmwareCommand::UpgradeConfirm {
                sequence_id,
                src_id,
            } => Self::UpgradeConfirm {
                sequence_id: sequence_id.clone(),
                src_id: *src_id,
            },
            FirmwareCommand::ConsistencyConfirm {
                sequence_id,
                src_id,
            } => Self::ConsistencyConfirm {
                sequence_id: sequence_id.clone(),
                src_id: *src_id,
            },
            FirmwareCommand::Start {
                sequence_id,
                src_id,
                module,
                version,
                ..
            } => Self::Start {
                sequence_id: sequence_id.clone(),
                src_id: *src_id,
                module: module.clone(),
                version: version.clone(),
            },
            FirmwareCommand::SwitchAmsFirmware {
                sequence_id,
                src_id,
                id,
            } => Self::SwitchAmsFirmware {
                sequence_id: sequence_id.clone(),
                src_id: *src_id,
                id: *id,
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FirmwareAcknowledgement {
    pub command: String,
    pub sequence_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(rename = "err_code", skip_serializing_if = "Option::is_none")]
    pub error_code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum FirmwareTerminalOutcome {
    Acknowledged {
        acknowledgement: FirmwareAcknowledgement,
    },
    PublishedWithoutAcknowledgement,
}
