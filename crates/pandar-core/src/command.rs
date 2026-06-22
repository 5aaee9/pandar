use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{AgentId, CommandId, CoreError, TenantId};

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
