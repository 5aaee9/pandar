mod agents;
mod tokens;
mod users;

use serde::Serialize;

use crate::{
    repositories::{User, UserIdentity, UserRole},
    routes::{AgentResponse, ApiError},
};

pub(super) use agents::{create_agent_pairing, revoke_agent_credential, rotate_agent_credential};
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
pub(super) struct AgentPairingResponse {
    agent: AgentResponse,
    agent_env: String,
}

#[derive(Debug, Serialize)]
pub(super) struct AgentCredentialRotateResponse {
    agent: AgentResponse,
    credential: String,
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
