use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(Uuid);

impl TenantId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(value: &str) -> Result<Self, CoreError> {
        Uuid::parse_str(value)
            .map(Self)
            .map_err(|_| CoreError::InvalidTenantId)
    }
}

impl Default for TenantId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(Uuid);

impl AgentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(value: &str) -> Result<Self, CoreError> {
        Uuid::parse_str(value)
            .map(Self)
            .map_err(|_| CoreError::InvalidAgentId)
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommandId(Uuid);

impl CommandId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(value: &str) -> Result<Self, CoreError> {
        Uuid::parse_str(value)
            .map(Self)
            .map_err(|_| CoreError::InvalidCommandId)
    }
}

impl Default for CommandId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CommandId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tenant {
    pub id: TenantId,
    pub slug: String,
    pub display_name: String,
    pub created_at: String,
}

impl Tenant {
    pub fn new(
        slug: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Result<Self, CoreError> {
        let slug = slug.into();
        if slug.trim().is_empty() {
            return Err(CoreError::EmptyTenantSlug);
        }

        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(CoreError::EmptyTenantDisplayName);
        }

        Self::from_parts(TenantId::new(), slug, display_name, created_at_now())
    }

    pub fn from_parts(
        id: TenantId,
        slug: impl Into<String>,
        display_name: impl Into<String>,
        created_at: impl Into<String>,
    ) -> Result<Self, CoreError> {
        let slug = slug.into();
        if slug.trim().is_empty() {
            return Err(CoreError::EmptyTenantSlug);
        }

        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(CoreError::EmptyTenantDisplayName);
        }

        Ok(Self {
            id,
            slug,
            display_name,
            created_at: created_at.into(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    Offline,
    Connecting,
    Online,
}

impl AgentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Connecting => "connecting",
            Self::Online => "online",
        }
    }
}

impl fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AgentStatus {
    type Err = CoreError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "offline" => Ok(Self::Offline),
            "connecting" => Ok(Self::Connecting),
            "online" => Ok(Self::Online),
            value => Err(CoreError::InvalidAgentStatus(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    pub id: AgentId,
    pub tenant_id: TenantId,
    pub name: String,
    pub status: AgentStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Printer {
    pub id: String,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub serial_number: String,
    pub name: String,
    pub model: Option<String>,
    pub status: String,
    pub last_seen_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterParts {
    pub id: String,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub serial_number: String,
    pub name: String,
    pub model: Option<String>,
    pub status: String,
    pub last_seen_at: String,
    pub created_at: String,
}

impl Printer {
    pub fn from_parts(parts: PrinterParts) -> Result<Self, CoreError> {
        required(&parts.id, CoreError::EmptyPrinterId)?;
        required(&parts.serial_number, CoreError::EmptyPrinterSerialNumber)?;
        required(&parts.name, CoreError::EmptyPrinterName)?;
        required(&parts.status, CoreError::EmptyPrinterStatus)?;

        Ok(Self {
            id: parts.id,
            tenant_id: parts.tenant_id,
            agent_id: parts.agent_id,
            serial_number: parts.serial_number,
            name: parts.name,
            model: parts.model.filter(|model| !model.trim().is_empty()),
            status: parts.status,
            last_seen_at: parts.last_seen_at,
            created_at: parts.created_at,
        })
    }
}

fn required(value: &str, error: CoreError) -> Result<(), CoreError> {
    (!value.trim().is_empty()).then_some(()).ok_or(error)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandStatus {
    Queued,
    Sent,
    Acknowledged,
    Succeeded,
    Failed,
}

impl CommandStatus {
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

impl fmt::Display for CommandStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CommandStatus {
    type Err = CoreError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "sent" => Ok(Self::Sent),
            "acknowledged" => Ok(Self::Acknowledged),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            value => Err(CoreError::InvalidCommandStatus(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRecord {
    pub id: CommandId,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub printer_id: Option<String>,
    pub kind: String,
    pub status: CommandStatus,
    pub payload_json: String,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRecordParts {
    pub id: CommandId,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub printer_id: Option<String>,
    pub kind: String,
    pub status: String,
    pub payload_json: String,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl CommandRecord {
    pub fn from_parts(parts: CommandRecordParts) -> Result<Self, CoreError> {
        let kind = parts.kind;
        if kind.trim().is_empty() {
            return Err(CoreError::EmptyCommandKind);
        }

        Ok(Self {
            id: parts.id,
            tenant_id: parts.tenant_id,
            agent_id: parts.agent_id,
            printer_id: parts.printer_id,
            kind,
            status: parts.status.parse()?,
            payload_json: parts.payload_json,
            error: parts.error,
            created_at: parts.created_at,
            updated_at: parts.updated_at,
        })
    }
}

impl Agent {
    #[rustfmt::skip]
    pub fn new(tenant_id: TenantId, name: impl Into<String>) -> Result<Self, CoreError> {
        Self::from_parts(AgentId::new(), tenant_id, name, AgentStatus::Offline, created_at_now())
    }

    pub fn from_parts(
        id: AgentId,
        tenant_id: TenantId,
        name: impl Into<String>,
        status: AgentStatus,
        created_at: impl Into<String>,
    ) -> Result<Self, CoreError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(CoreError::EmptyAgentName);
        }

        Ok(Self {
            id,
            tenant_id,
            name,
            status,
            created_at: created_at.into(),
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("tenant id must be a UUID")]
    InvalidTenantId,
    #[error("agent id must be a UUID")]
    InvalidAgentId,
    #[error("command id must be a UUID")]
    InvalidCommandId,
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
    #[error("invalid agent status: {0}")]
    InvalidAgentStatus(String),
    #[error("invalid command status: {0}")]
    InvalidCommandStatus(String),
}

pub fn created_at_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("RFC3339 formatting should succeed")
}

#[cfg(test)]
mod tests;
