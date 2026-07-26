use pandar_core::TenantId;
use serde::Serialize;

use crate::repositories::{RepositoryError, RepositoryResult, User};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TenantTokenScope {
    All,
    AgentRegister,
    PluginStudio,
    MobileSession,
}

impl TenantTokenScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "*",
            Self::AgentRegister => "agent:register",
            Self::PluginStudio => "plugin:studio",
            Self::MobileSession => "mobile:session",
        }
    }

    pub fn parse(value: &str) -> RepositoryResult<Self> {
        match value {
            "*" => Ok(Self::All),
            "agent:register" => Ok(Self::AgentRegister),
            "plugin:studio" => Ok(Self::PluginStudio),
            "mobile:session" => Ok(Self::MobileSession),
            other => Err(RepositoryError::InvalidTokenScope(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TenantToken {
    pub id: String,
    pub tenant_id: TenantId,
    pub name: String,
    pub scopes: Vec<TenantTokenScope>,
    pub created_by_user_id: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
}

impl TenantToken {
    pub fn has_scope(&self, scope: TenantTokenScope) -> bool {
        self.scopes.contains(&TenantTokenScope::All) || self.scopes.contains(&scope)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantTokenWithPlaintext {
    pub token: TenantToken,
    pub plaintext_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedTenantToken {
    pub token: TenantToken,
    pub session_user: Option<User>,
}
