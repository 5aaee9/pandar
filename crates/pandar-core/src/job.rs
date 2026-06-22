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
pub struct Job {
    pub id: JobId,
    pub tenant_id: TenantId,
    pub printer_id: String,
    pub agent_id: AgentId,
    pub artifact_id: String,
    pub command_id: CommandId,
    pub status: JobStatus,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
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
            created_at: parts.created_at,
            updated_at: parts.updated_at,
        })
    }
}
