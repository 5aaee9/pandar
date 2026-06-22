use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{AgentId, CommandId, CoreError, JobId, TenantId, required};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Queued,
    Sent,
    Acknowledged,
    Succeeded,
    Failed,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Sent => "sent",
            Self::Acknowledged => "acknowledged",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

impl fmt::Display for JobStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for JobStatus {
    type Err = CoreError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "sent" => Ok(Self::Sent),
            "acknowledged" => Ok(Self::Acknowledged),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            value => Err(CoreError::InvalidJobStatus(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrintStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl PrintStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for PrintStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PrintStatus {
    type Err = CoreError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            value => Err(CoreError::InvalidPrintStatus(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobArtifact {
    pub id: String,
    pub tenant_id: TenantId,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub storage_path: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobArtifactParts {
    pub id: String,
    pub tenant_id: TenantId,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub storage_path: String,
    pub created_at: String,
}

impl JobArtifact {
    pub fn from_parts(parts: JobArtifactParts) -> Result<Self, CoreError> {
        required(&parts.id, CoreError::EmptyArtifactId)?;
        required(&parts.filename, CoreError::EmptyArtifactFilename)?;
        required(&parts.content_type, CoreError::EmptyArtifactContentType)?;
        required(&parts.storage_path, CoreError::EmptyArtifactStoragePath)?;
        if parts.size_bytes == 0 {
            return Err(CoreError::EmptyArtifactBody);
        }

        Ok(Self {
            id: parts.id,
            tenant_id: parts.tenant_id,
            filename: parts.filename,
            content_type: parts.content_type,
            size_bytes: parts.size_bytes,
            storage_path: parts.storage_path,
            created_at: parts.created_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobPrintState {
    pub status: PrintStatus,
    pub printer_state: Option<String>,
    pub progress_percent: Option<u8>,
    pub remaining_time_minutes: Option<u32>,
    pub current_layer: Option<u32>,
    pub total_layers: Option<u32>,
    pub active_file: Option<String>,
    pub last_progress_percent: Option<u8>,
    pub last_layer: Option<u32>,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub updated_at: Option<String>,
}

impl JobPrintState {
    pub fn pending() -> Self {
        Self {
            status: PrintStatus::Pending,
            printer_state: None,
            progress_percent: None,
            remaining_time_minutes: None,
            current_layer: None,
            total_layers: None,
            active_file: None,
            last_progress_percent: None,
            last_layer: None,
            error: None,
            started_at: None,
            finished_at: None,
            updated_at: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub tenant_id: TenantId,
    pub printer_id: String,
    pub agent_id: AgentId,
    pub artifact_id: String,
    pub command_id: CommandId,
    pub status: JobStatus,
    pub error: Option<String>,
    pub print: JobPrintState,
    pub ams_mapping_json: Option<String>,
    pub ams_mapping2_json: Option<String>,
    pub filament_usage: Vec<JobFilamentUsage>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobFilamentUsage {
    pub id: String,
    pub tenant_id: TenantId,
    pub job_id: JobId,
    pub slot_index: u32,
    pub source: String,
    pub ams_id: Option<String>,
    pub tray_id: Option<String>,
    pub global_tray_id: Option<u32>,
    pub external_id: Option<String>,
    pub filament_id: Option<String>,
    pub setting_id: Option<String>,
    pub filament_type: Option<String>,
    pub color: Option<String>,
    pub used_mm: Option<String>,
    pub used_grams: Option<String>,
    pub confidence: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobParts {
    pub id: JobId,
    pub tenant_id: TenantId,
    pub printer_id: String,
    pub agent_id: AgentId,
    pub artifact_id: String,
    pub command_id: CommandId,
    pub status: String,
    pub error: Option<String>,
    pub print_status: String,
    pub printer_state: Option<String>,
    pub progress_percent: Option<u8>,
    pub remaining_time_minutes: Option<u32>,
    pub current_layer: Option<u32>,
    pub total_layers: Option<u32>,
    pub active_file: Option<String>,
    pub last_progress_percent: Option<u8>,
    pub last_layer: Option<u32>,
    pub print_error: Option<String>,
    pub print_started_at: Option<String>,
    pub print_finished_at: Option<String>,
    pub print_updated_at: Option<String>,
    pub ams_mapping_json: Option<String>,
    pub ams_mapping2_json: Option<String>,
    pub filament_usage: Vec<JobFilamentUsage>,
    pub created_at: String,
    pub updated_at: String,
}

impl Job {
    pub fn from_parts(parts: JobParts) -> Result<Self, CoreError> {
        required(&parts.printer_id, CoreError::EmptyJobPrinterId)?;
        required(&parts.artifact_id, CoreError::EmptyJobArtifactId)?;

        Ok(Self {
            id: parts.id,
            tenant_id: parts.tenant_id,
            printer_id: parts.printer_id,
            agent_id: parts.agent_id,
            artifact_id: parts.artifact_id,
            command_id: parts.command_id,
            status: parts.status.parse()?,
            error: parts.error,
            print: JobPrintState {
                status: parts.print_status.parse()?,
                printer_state: parts.printer_state,
                progress_percent: parts.progress_percent,
                remaining_time_minutes: parts.remaining_time_minutes,
                current_layer: parts.current_layer,
                total_layers: parts.total_layers,
                active_file: parts.active_file,
                last_progress_percent: parts.last_progress_percent,
                last_layer: parts.last_layer,
                error: parts.print_error,
                started_at: parts.print_started_at,
                finished_at: parts.print_finished_at,
                updated_at: parts.print_updated_at,
            },
            ams_mapping_json: parts.ams_mapping_json,
            ams_mapping2_json: parts.ams_mapping2_json,
            filament_usage: parts.filament_usage,
            created_at: parts.created_at,
            updated_at: parts.updated_at,
        })
    }
}
