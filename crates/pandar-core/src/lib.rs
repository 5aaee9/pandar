use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub mod agent;
pub mod ams;
pub mod command;
pub mod compatibility;
pub mod cooling;
pub mod device_features;
pub mod firmware;
pub mod h2c;
pub mod ids;
pub mod job;
pub mod print_error;
pub mod print_transfer;
pub mod printer;
pub mod studio_print;
pub mod tenant;

pub use agent::{Agent, AgentStatus};
pub use ams::AmsUnitKind;
pub use command::{CommandRecord, CommandRecordParts, CommandStatus};
pub use cooling::{
    PrinterCoolingFan, PrinterCoolingFanKind, PrinterCoolingMode, PrinterCoolingSystem,
};
pub use device_features::{
    BambuDeviceFeature, BambuDeviceFeatures, BambuDeviceFeaturesParseError, RequiredDeviceFeature,
};
pub use firmware::{
    AmsFirmwareDescriptor, AmsFirmwareSwitchState, FirmwareAcknowledgement, FirmwareCatalogEntry,
    FirmwareCatalogTarget, FirmwareCommand, FirmwareControlMetadata, FirmwareTerminalOutcome,
    PrinterFirmwareModule, PrinterFirmwareState, PrinterFirmwareStatus, PrinterFirmwareVersion,
    PrinterUpgradeState,
};
pub use h2c::{
    BambuNozzleDevice, BambuNozzleHolder, BambuNozzleInfo, BambuNozzleSystem,
    H2cAutoMappingFilamentInfo, H2cAutoMappingGroupInfo, H2cAutoMappingNozzleInfo,
    H2cAutoNozzleMappingEnvelope, H2cAutoNozzleMappingRequest, H2cAutoNozzleMappingResponse,
    H2cAutoNozzleMappingResponseEnvelope, valid_h2c_nozzle_mapping, valid_physical_nozzle_id,
};
pub use ids::{AgentId, CommandId, JobId, TenantId};
pub use job::{
    Job, JobArtifact, JobArtifactParts, JobFilamentUsage, JobParts, JobPrintState, JobStatus,
    PrintCalibrationMode, PrintStatus,
};
pub use print_error::PrintErrorAction;
pub use print_transfer::{PrintTransferFailure, PrintTransferPhase};
pub use printer::{Printer, PrinterNozzleTemperature, PrinterParts};
pub use studio_print::{
    StudioAmsMappingEntry, StudioAmsMappingInfo, StudioFiniteF64, StudioNozzleInfo,
    StudioPrintMetadata, StudioPrintMetadataV1, StudioSubmissionId,
};
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
    #[error("invalid print calibration mode: {0}")]
    InvalidPrintCalibrationMode(u8),
    #[error("studio submission id must be a positive int32: {0}")]
    InvalidStudioSubmissionId(i64),
    #[error("Studio print numeric metadata must be finite")]
    NonFiniteStudioNumber,
    #[error("invalid Studio print metadata: {0}")]
    InvalidStudioPrintMetadata(String),
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
