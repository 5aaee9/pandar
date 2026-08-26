use pandar_core::{AgentId, TenantId};

use crate::{
    AppState,
    repositories::{PrinterOperationKind, PrinterOperationPayload},
    sessions::SessionToken,
};
use pandar_protocol::agent::v1::AgentCapability;

pub(super) async fn queued_command_gate_failure(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    current: bool,
    operation: Option<&PrinterOperationPayload>,
) -> Option<String> {
    if !current
        || !matches!(
            operation.map(|payload| &payload.operation),
            Some(PrinterOperationKind::GcodeLine { .. })
        )
    {
        return None;
    }
    (state.sessions().current_token_for_capability(
        tenant_id,
        agent_id,
        AgentCapability::GcodeLine,
    ).await != Some(token))
        .then(|| {
            "agent capability gate failed: current agent session does not advertise gcode-line capability"
                .to_owned()
        })
}
