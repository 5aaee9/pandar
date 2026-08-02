use pandar_core::{AgentId, TenantId};
use tokio::sync::mpsc;
use tonic::Status;

use crate::protocol::agent::v1::AgentCapability;
use crate::protocol::agent::v1::HubCommand;

use super::SessionRegistry;

impl SessionRegistry {
    pub async fn transient_command_sender(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> Option<mpsc::Sender<Result<HubCommand, Status>>> {
        self.sessions
            .lock()
            .await
            .get(&agent_id)
            .filter(|session| session.tenant_id == tenant_id)
            .map(|session| session.command_sender.clone())
    }

    pub async fn transient_command_sender_for_capability(
        &self,
        tenant_id: TenantId,
        agent_id: AgentId,
        capability: AgentCapability,
    ) -> Option<mpsc::Sender<Result<HubCommand, Status>>> {
        self.sessions
            .lock()
            .await
            .get(&agent_id)
            .filter(|session| {
                session.tenant_id == tenant_id && session.capabilities.contains(&capability)
            })
            .map(|session| session.command_sender.clone())
    }
}
