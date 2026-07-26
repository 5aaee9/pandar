use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex},
};

use pandar_core::{AgentId, TenantId};

const MAX_AUTHENTICATED_CONNECTIONS_PER_AGENT: usize = 8;
const MAX_AUTHENTICATED_CONNECTIONS_PER_TENANT: usize = 128;

pub(super) struct PeerConnections {
    counts: Mutex<ConnectionCounts>,
    max_per_peer: usize,
}

#[derive(Default)]
struct ConnectionCounts {
    peers: HashMap<IpAddr, usize>,
    agents: HashMap<(TenantId, AgentId), usize>,
    tenants: HashMap<TenantId, usize>,
}

impl PeerConnections {
    pub(super) fn new(max_per_peer: usize) -> Arc<Self> {
        Arc::new(Self {
            counts: Mutex::new(ConnectionCounts::default()),
            max_per_peer,
        })
    }

    pub(super) fn try_acquire(self: &Arc<Self>, peer: IpAddr) -> Option<PeerPermit> {
        let mut counts = self.counts.lock().expect("gRPC connection counts");
        let count = counts.peers.entry(peer).or_default();
        if *count >= self.max_per_peer {
            return None;
        }
        *count += 1;
        Some(PeerPermit(Arc::new(ConnectionSlot {
            connections: Arc::clone(self),
            state: Mutex::new(ConnectionSlotState::Setup(peer)),
        })))
    }

    fn authenticate(
        &self,
        state: &mut ConnectionSlotState,
        tenant_id: TenantId,
        agent_id: AgentId,
    ) -> bool {
        match state {
            ConnectionSlotState::Authenticated {
                tenant_id: current_tenant,
                agent_id: current_agent,
            } => return *current_tenant == tenant_id && *current_agent == agent_id,
            ConnectionSlotState::Setup(_) => {}
        }
        let mut counts = self.counts.lock().expect("gRPC connection counts");
        if counts
            .agents
            .get(&(tenant_id, agent_id))
            .copied()
            .unwrap_or_default()
            >= MAX_AUTHENTICATED_CONNECTIONS_PER_AGENT
            || counts.tenants.get(&tenant_id).copied().unwrap_or_default()
                >= MAX_AUTHENTICATED_CONNECTIONS_PER_TENANT
        {
            return false;
        }
        let ConnectionSlotState::Setup(peer) = *state else {
            unreachable!("authenticated gRPC slot returned above")
        };
        decrement(&mut counts.peers, &peer);
        *counts.agents.entry((tenant_id, agent_id)).or_default() += 1;
        *counts.tenants.entry(tenant_id).or_default() += 1;
        *state = ConnectionSlotState::Authenticated {
            tenant_id,
            agent_id,
        };
        true
    }

    fn release(&self, state: &ConnectionSlotState) {
        let mut counts = self.counts.lock().expect("gRPC connection counts");
        match *state {
            ConnectionSlotState::Setup(peer) => decrement(&mut counts.peers, &peer),
            ConnectionSlotState::Authenticated {
                tenant_id,
                agent_id,
            } => {
                decrement(&mut counts.agents, &(tenant_id, agent_id));
                decrement(&mut counts.tenants, &tenant_id);
            }
        }
    }
}

fn decrement<K>(counts: &mut HashMap<K, usize>, key: &K)
where
    K: Eq + std::hash::Hash,
{
    let count = counts.get_mut(key).expect("gRPC connection permit count");
    *count -= 1;
    if *count == 0 {
        counts.remove(key);
    }
}

#[derive(Clone)]
pub(super) struct PeerPermit(Arc<ConnectionSlot>);

impl PeerPermit {
    pub(super) fn mark_authenticated(&self, tenant_id: TenantId, agent_id: AgentId) -> bool {
        let mut state = self.0.state.lock().expect("gRPC connection slot state");
        self.0
            .connections
            .authenticate(&mut state, tenant_id, agent_id)
    }
}

struct ConnectionSlot {
    connections: Arc<PeerConnections>,
    state: Mutex<ConnectionSlotState>,
}

#[derive(Clone, Copy)]
enum ConnectionSlotState {
    Setup(IpAddr),
    Authenticated {
        tenant_id: TenantId,
        agent_id: AgentId,
    },
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.connections
            .release(self.state.get_mut().expect("gRPC connection slot state"));
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::*;

    #[test]
    fn authenticated_connections_are_bounded_per_agent() {
        let connections = PeerConnections::new(64);
        let tenant_id = TenantId::parse("00000000-0000-0000-0000-000000000001").unwrap();
        let agent_id = AgentId::parse("00000000-0000-0000-0000-000000000002").unwrap();
        let permits = (0..=MAX_AUTHENTICATED_CONNECTIONS_PER_AGENT)
            .map(|_| {
                connections
                    .try_acquire(IpAddr::V4(Ipv4Addr::LOCALHOST))
                    .unwrap()
            })
            .collect::<Vec<_>>();

        for permit in &permits[..MAX_AUTHENTICATED_CONNECTIONS_PER_AGENT] {
            assert!(permit.mark_authenticated(tenant_id, agent_id));
        }
        assert!(
            !permits[MAX_AUTHENTICATED_CONNECTIONS_PER_AGENT]
                .mark_authenticated(tenant_id, agent_id)
        );
    }
}
