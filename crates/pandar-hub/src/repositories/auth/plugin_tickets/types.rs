use pandar_core::TenantId;
use serde::Serialize;

use crate::repositories::{RepositoryResult, TenantTokenWithPlaintext};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginLoginTicket {
    pub id: String,
    pub tenant_id: TenantId,
    pub user_id: Option<String>,
    pub redirect_url: String,
    pub kind: LoginTicketKind,
    pub code_challenge: Option<String>,
    pub created_at: String,
    pub expires_at: String,
    pub used_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LoginTicketKind {
    Plugin,
    Mobile,
}

impl LoginTicketKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Plugin => "plugin",
            Self::Mobile => "mobile",
        }
    }

    pub(super) fn parse(value: &str) -> RepositoryResult<Self> {
        match value {
            "plugin" => Ok(Self::Plugin),
            "mobile" => Ok(Self::Mobile),
            other => Err(anyhow::anyhow!("invalid persisted login ticket kind {other}").into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLoginTicketWithPlaintext {
    pub ticket: PluginLoginTicket,
    pub plaintext_ticket: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLoginTicketExchange {
    pub ticket: PluginLoginTicket,
    pub redirect_url: String,
    pub tenant_token: TenantTokenWithPlaintext,
}
