use pandar_core::{AgentId, TenantId};

use crate::{
    AppState,
    protocol::agent::v1::AgentCapability,
    repositories::{PrinterOperationKind, PrinterOperationPayload},
    sessions::SessionToken,
};

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
