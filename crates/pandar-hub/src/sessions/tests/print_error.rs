use super::*;
use crate::{
    protocol::agent::v1::{AgentCapability, LinkPrinter, hub_command},
    repositories::{AuditActor, LinkPrinterPayload},
    sessions::live_commands::fail_pending_live_commands,
};

mod capabilities;
mod claims;

fn session_with_capabilities(
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    command_sender: mpsc::Sender<Result<HubCommand, Status>>,
    capabilities: impl IntoIterator<Item = AgentCapability>,
) -> AgentSession {
    AgentSession {
        token,
        tenant_id,
        agent_id,
        name: "agent".to_owned(),
        version: "0.1.0".to_owned(),
        connected_at: "2026-07-10T00:00:00Z".to_owned(),
        last_heartbeat_at: "2026-07-10T00:00:00Z".to_owned(),
        wake_sender: mpsc::channel(1).0,
        close_sender: mpsc::channel(1).0,
        command_sender,
        capabilities: capabilities.into_iter().collect(),
        pending_live_commands: empty_pending_live_commands(),
        live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
    }
}

fn capable_session(
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    command_sender: mpsc::Sender<Result<HubCommand, Status>>,
) -> AgentSession {
    session_with_capabilities(
        tenant_id,
        agent_id,
        token,
        command_sender,
        [AgentCapability::HandlePrintError],
    )
}

fn link_command(command_id: CommandId, access_code: &str) -> HubCommand {
    HubCommand {
        command_id: command_id.to_string(),
        command: Some(hub_command::Command::LinkPrinter(LinkPrinter {
            host: "192.0.2.10".to_owned(),
            access_code: access_code.to_owned(),
            name: String::new(),
            printer_type: "BambuLab".to_owned(),
        })),
    }
}

async fn live_link_fixture(
    slug: &str,
) -> (AppState, TenantId, AgentId, pandar_core::CommandRecord) {
    let state = AppState::sqlite_for_tests().await.unwrap();
    let tenant = state.tenants().create(slug, slug).await.unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant.id,
            agent.id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: "SECRET-LINK-CODE".to_owned(),
                name: None,
            },
            AuditActor::tenant_token(None, "slice-b", vec!["*"]),
        )
        .await
        .unwrap();
    (state, tenant.id, agent.id, command)
}
