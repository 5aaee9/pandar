use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{AgentId, CoreError, TenantId, created_at_now};

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
