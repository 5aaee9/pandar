mod agents;
mod tokens;
mod users;

use serde::Serialize;

use crate::{
    repositories::{ApiToken, User, UserIdentity, UserRole},
    routes::{AgentResponse, ApiError},
};

pub(super) use agents::create_agent_pairing;
pub(super) use tokens::{create_api_token, list_api_tokens, revoke_api_token};
pub(super) use users::{
    create_user, link_user_identity, list_user_identities, list_users, update_user_role,
};

#[derive(Debug, Serialize)]
pub(super) struct UserResponse {
    id: String,
    tenant_id: String,
    email: String,
    display_name: String,
    role: &'static str,
    created_at: String,
}

#[derive(Debug, Serialize)]
pub(super) struct UserListResponse {
    users: Vec<UserResponse>,
}

#[derive(Debug, Serialize)]
pub(super) struct UserIdentityResponse {
    id: String,
    tenant_id: String,
    user_id: String,
    provider: String,
    subject: String,
    created_at: String,
}

#[derive(Debug, Serialize)]
pub(super) struct UserIdentityListResponse {
    identities: Vec<UserIdentityResponse>,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiTokenResponse {
    id: String,
    tenant_id: String,
    user_id: String,
    name: String,
    created_at: String,
    last_used_at: Option<String>,
    revoked_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiTokenWithPlaintextResponse {
    #[serde(flatten)]
    api_token: ApiTokenResponse,
    token: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiTokenListResponse {
    api_tokens: Vec<ApiTokenResponse>,
}

#[derive(Debug, Serialize)]
pub(super) struct AgentPairingResponse {
    agent: AgentResponse,
    agent_env: String,
}

fn parse_user_role(role: &str) -> Result<UserRole, ApiError> {
    UserRole::parse(role).map_err(|_| ApiError::bad_request("invalid_user_role"))
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            tenant_id: user.tenant_id.to_string(),
            email: user.email,
            display_name: user.display_name,
            role: user.role.as_str(),
            created_at: user.created_at,
        }
    }
}

impl From<UserIdentity> for UserIdentityResponse {
    fn from(identity: UserIdentity) -> Self {
        Self {
            id: identity.id,
            tenant_id: identity.tenant_id.to_string(),
            user_id: identity.user_id,
            provider: identity.provider,
            subject: identity.subject,
            created_at: identity.created_at,
        }
    }
}

impl From<ApiToken> for ApiTokenResponse {
    fn from(token: ApiToken) -> Self {
        Self {
            id: token.id,
            tenant_id: token.tenant_id.to_string(),
            user_id: token.user_id,
            name: token.name,
            created_at: token.created_at,
            last_used_at: token.last_used_at,
            revoked_at: token.revoked_at,
        }
    }
}

impl ApiTokenWithPlaintextResponse {
    fn new(token: ApiToken, plaintext_token: String) -> Self {
        Self {
            api_token: ApiTokenResponse::from(token),
            token: plaintext_token,
        }
    }
}
