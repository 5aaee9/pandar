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

impl Agent {
    pub fn new(tenant_id: TenantId, name: impl Into<String>) -> Result<Self, CoreError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(CoreError::EmptyAgentName);
        }

        Self::from_parts(
            AgentId::new(),
            tenant_id,
            name,
            AgentStatus::Offline,
            created_at_now(),
        )
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
    #[error("tenant slug cannot be empty")]
    EmptyTenantSlug,
    #[error("tenant display name cannot be empty")]
    EmptyTenantDisplayName,
    #[error("agent name cannot be empty")]
    EmptyAgentName,
    #[error("invalid agent status: {0}")]
    InvalidAgentStatus(String),
}

fn created_at_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("RFC3339 formatting should succeed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenant_id_parse_rejects_invalid_uuid() {
        let err = TenantId::parse("not-a-uuid").unwrap_err();

        assert_eq!(err, CoreError::InvalidTenantId);
    }

    #[test]
    fn agent_id_parse_rejects_invalid_uuid() {
        let err = AgentId::parse("not-a-uuid").unwrap_err();

        assert_eq!(err, CoreError::InvalidAgentId);
    }

    #[test]
    fn tenant_requires_slug() {
        let err = Tenant::new(" ", "Acme").unwrap_err();

        assert_eq!(err, CoreError::EmptyTenantSlug);
    }

    #[test]
    fn tenant_requires_display_name() {
        let err = Tenant::new("acme", " ").unwrap_err();

        assert_eq!(err, CoreError::EmptyTenantDisplayName);
    }

    #[test]
    fn tenant_new_sets_iso_utc_created_at() {
        let tenant = Tenant::new("acme", "Acme").unwrap();

        assert!(tenant.created_at.ends_with('Z'));
        assert!(OffsetDateTime::parse(&tenant.created_at, &Rfc3339).is_ok());
    }

    #[test]
    fn tenant_from_parts_requires_display_name() {
        let err =
            Tenant::from_parts(TenantId::new(), "acme", " ", "2026-06-20T00:00:00Z").unwrap_err();

        assert_eq!(err, CoreError::EmptyTenantDisplayName);
    }

    #[test]
    fn agent_requires_name() {
        let err = Agent::new(TenantId::new(), " ").unwrap_err();

        assert_eq!(err, CoreError::EmptyAgentName);
    }

    #[test]
    fn agent_starts_offline_for_a_tenant() {
        let tenant = Tenant::new("acme", "Acme").unwrap();
        let agent = Agent::new(tenant.id, "garage").unwrap();

        assert_eq!(agent.tenant_id, tenant.id);
        assert_eq!(agent.status, AgentStatus::Offline);
    }

    #[test]
    fn agent_status_round_trips_persisted_strings() {
        assert_eq!(AgentStatus::Offline.as_str(), "offline");
        assert_eq!(AgentStatus::Connecting.as_str(), "connecting");
        assert_eq!(AgentStatus::Online.as_str(), "online");
        assert_eq!("offline".parse(), Ok(AgentStatus::Offline));
        assert_eq!("connecting".parse(), Ok(AgentStatus::Connecting));
        assert_eq!("online".parse(), Ok(AgentStatus::Online));
        assert_eq!(
            "retired".parse::<AgentStatus>(),
            Err(CoreError::InvalidAgentStatus("retired".to_string()))
        );
    }
}
