use anyhow::Context;
use pandar_core::{Agent, AgentId, AgentStatus, TenantId};

use crate::{
    entities::agents,
    repositories::{RepositoryError, RepositoryResult},
};

use super::AgentCredentialRecord;

pub(super) fn agent_from_model(model: agents::Model) -> RepositoryResult<Agent> {
    let status = model
        .status
        .parse::<AgentStatus>()
        .map_err(|_| RepositoryError::InvalidPersistedStatus(model.status.clone()))?;
    Agent::from_parts(
        AgentId::parse(&model.id).map_err(anyhow::Error::from)?,
        TenantId::parse(&model.tenant_id).map_err(anyhow::Error::from)?,
        model.name,
        status,
        model.created_at,
    )
    .map_err(anyhow::Error::from)
    .context("failed to rehydrate agent")
    .map_err(RepositoryError::from)
}

pub(super) fn agent_credential_from_model(
    model: agents::Model,
) -> RepositoryResult<AgentCredentialRecord> {
    let credential_hash = model.credential_hash.clone();
    let credential_rotated_at = model.credential_rotated_at.clone();
    let credential_revoked_at = model.credential_revoked_at.clone();
    Ok(AgentCredentialRecord {
        agent: agent_from_model(model)?,
        credential_hash,
        credential_rotated_at,
        credential_revoked_at,
    })
}
