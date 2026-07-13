use pandar_core::{
    CommandId, FirmwareTerminalOutcome, PrinterFirmwareModule, PrinterFirmwareStatus,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PreparedFirmwareControl {
    pub command_id: CommandId,
    pub prepared_token: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FirmwareExecutePhase {
    PrePublishFailure,
    Acknowledged,
    Rejected,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FirmwareExecuteResult {
    pub command_id: CommandId,
    pub phase: FirmwareExecutePhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<FirmwareTerminalOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transient_status: Option<PrinterFirmwareStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FirmwareRefreshResult {
    pub command_id: CommandId,
    pub modules: Vec<PrinterFirmwareModule>,
    pub module_revision: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum FirmwareServiceError {
    #[error("firmware_control_unavailable")]
    Unavailable,
    #[error("invalid firmware prepared token")]
    InvalidPreparedToken,
    #[error("prepared firmware command does not match execute command")]
    MetadataMismatch,
    #[error("firmware command failed before publish: {message}")]
    CommandFailed { message: String },
    #[error("firmware service failed")]
    Internal {
        #[source]
        source: anyhow::Error,
    },
    #[error("firmware service failed before publish")]
    InternalPrePublish {
        #[source]
        source: anyhow::Error,
    },
}

impl FirmwareServiceError {
    pub(crate) fn internal(error: impl Into<anyhow::Error>) -> Self {
        Self::Internal {
            source: error.into(),
        }
    }

    pub(crate) fn internal_pre_publish(error: impl Into<anyhow::Error>) -> Self {
        Self::InternalPrePublish {
            source: error.into(),
        }
    }

    pub(crate) fn into_pre_publish(self) -> Self {
        match self {
            Self::Internal { source } | Self::InternalPrePublish { source } => {
                Self::InternalPrePublish { source }
            }
            other => other,
        }
    }
}
