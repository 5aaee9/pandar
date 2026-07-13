use std::fmt;

use pandar_core::FirmwareControlMetadata;

#[derive(Clone, Eq, PartialEq)]
pub enum StudioFirmwareCommand {
    UpgradeConfirm {
        sequence_id: String,
        src_id: i64,
    },
    ConsistencyConfirm {
        sequence_id: String,
        src_id: i64,
    },
    Start {
        sequence_id: String,
        src_id: i64,
        url: String,
        module: String,
        version: String,
    },
    McForAmsFirmwareUpgrade {
        sequence_id: String,
        src_id: i64,
        id: i32,
    },
}

#[derive(Debug, Eq, PartialEq)]
pub enum StudioFirmwareParse {
    NotFirmware,
    Firmware(StudioFirmwareCommand),
    InvalidFirmware,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FirmwareSendOutcome {
    Acknowledged,
    Rejected,
    PublishedWithoutAcknowledgement,
    OutcomeUnknown,
    PrePublishFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FirmwareSendResult {
    pub outcome: FirmwareSendOutcome,
    pub callback_token: Option<u64>,
}

impl StudioFirmwareCommand {
    pub(super) fn sequence_id(&self) -> &str {
        match self {
            Self::UpgradeConfirm { sequence_id, .. }
            | Self::ConsistencyConfirm { sequence_id, .. }
            | Self::Start { sequence_id, .. }
            | Self::McForAmsFirmwareUpgrade { sequence_id, .. } => sequence_id,
        }
    }

    pub(super) fn command_name(&self) -> &'static str {
        match self {
            Self::UpgradeConfirm { .. } => "upgrade_confirm",
            Self::ConsistencyConfirm { .. } => "consistency_confirm",
            Self::Start { .. } => "start",
            Self::McForAmsFirmwareUpgrade { .. } => "mc_for_ams_firmware_upgrade",
        }
    }
}

impl From<&StudioFirmwareCommand> for FirmwareControlMetadata {
    fn from(command: &StudioFirmwareCommand) -> Self {
        match command {
            StudioFirmwareCommand::UpgradeConfirm {
                sequence_id,
                src_id,
            } => Self::UpgradeConfirm {
                sequence_id: sequence_id.clone(),
                src_id: *src_id,
            },
            StudioFirmwareCommand::ConsistencyConfirm {
                sequence_id,
                src_id,
            } => Self::ConsistencyConfirm {
                sequence_id: sequence_id.clone(),
                src_id: *src_id,
            },
            StudioFirmwareCommand::Start {
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
            StudioFirmwareCommand::McForAmsFirmwareUpgrade {
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

impl fmt::Debug for StudioFirmwareCommand {
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
            Self::McForAmsFirmwareUpgrade {
                sequence_id,
                src_id,
                id,
            } => formatter
                .debug_struct("McForAmsFirmwareUpgrade")
                .field("sequence_id", sequence_id)
                .field("src_id", src_id)
                .field("id", id)
                .finish(),
        }
    }
}
