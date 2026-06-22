use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub mod agent;
pub mod command;
pub mod ids;
pub mod job;
pub mod printer;
pub mod tenant;

pub use agent::{Agent, AgentStatus};
pub use command::{CommandRecord, CommandRecordParts, CommandStatus};
pub use ids::{AgentId, CommandId, JobId, TenantId};
pub use job::{
    Job, JobArtifact, JobArtifactParts, JobParts, JobPrintState, JobStatus, PrintStatus,
};
pub use printer::{Printer, PrinterParts};
pub use tenant::Tenant;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("tenant id must be a UUID")]
    InvalidTenantId,
    #[error("agent id must be a UUID")]
    InvalidAgentId,
    #[error("command id must be a UUID")]
    InvalidCommandId,
    #[error("job id must be a UUID")]
    InvalidJobId,
    #[error("tenant slug cannot be empty")]
    EmptyTenantSlug,
    #[error("tenant display name cannot be empty")]
    EmptyTenantDisplayName,
    #[error("agent name cannot be empty")]
    EmptyAgentName,
    #[error("printer id cannot be empty")]
    EmptyPrinterId,
    #[error("printer serial number cannot be empty")]
    EmptyPrinterSerialNumber,
    #[error("printer name cannot be empty")]
    EmptyPrinterName,
    #[error("printer status cannot be empty")]
    EmptyPrinterStatus,
    #[error("command kind cannot be empty")]
    EmptyCommandKind,
    #[error("artifact id cannot be empty")]
    EmptyArtifactId,
    #[error("artifact filename cannot be empty")]
    EmptyArtifactFilename,
    #[error("artifact content type cannot be empty")]
    EmptyArtifactContentType,
    #[error("artifact storage path cannot be empty")]
    EmptyArtifactStoragePath,
    #[error("job printer id cannot be empty")]
    EmptyJobPrinterId,
    #[error("job artifact id cannot be empty")]
    EmptyJobArtifactId,
    #[error("artifact body cannot be empty")]
    EmptyArtifactBody,
    #[error("invalid agent status: {0}")]
    InvalidAgentStatus(String),
    #[error("invalid command status: {0}")]
    InvalidCommandStatus(String),
    #[error("invalid job status: {0}")]
    InvalidJobStatus(String),
    #[error("invalid print status: {0}")]
    InvalidPrintStatus(String),
}

pub fn created_at_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("RFC3339 formatting should succeed")
}

pub(crate) fn required(value: &str, error: CoreError) -> Result<(), CoreError> {
    (!value.trim().is_empty()).then_some(()).ok_or(error)
}

#[cfg(test)]
mod tests;
