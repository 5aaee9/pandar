use super::*;
use crate::AppState;
use pandar_protocol::agent::v1::{LinkPrinter, hub_command};

mod print_error;

fn test_session(
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    command_sender: mpsc::Sender<Result<HubCommand, Status>>,
) -> AgentSession {
    let (wake_sender, _) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);
    AgentSession {
        token,
        tenant_id,
        agent_id,
        name: "agent".to_string(),
        version: "0.1.0".to_string(),
        connected_at: "2026-06-20T00:00:00Z".to_string(),
        last_heartbeat_at: "2026-06-20T00:00:00Z".to_string(),
        wake_sender,
        close_sender,
        command_sender,
        capabilities: std::collections::HashSet::new(),
        pending_live_commands: empty_pending_live_commands(),
        live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
    }
}

fn test_command(command_id: CommandId) -> HubCommand {
    HubCommand {
        command_id: command_id.to_string(),
        command: None,
    }
}

#[tokio::test]
async fn sessions_register_touch_and_remove() {
    let registry = SessionRegistry::new();
    let (wake_sender, _) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();

    registry
        .register(AgentSession {
            token: SessionToken::new(),
            tenant_id,
            agent_id,
            name: "agent".to_string(),
            version: "0.1.0".to_string(),
            connected_at: "2026-06-20T00:00:00Z".to_string(),
            last_heartbeat_at: "2026-06-20T00:00:00Z".to_string(),
            wake_sender,
            close_sender,
            command_sender: mpsc::channel(1).0,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;

    let touched = registry
        .touch_heartbeat(agent_id, "2026-06-20T00:00:10Z")
        .await
        .unwrap();
    assert_eq!(touched.last_heartbeat_at, "2026-06-20T00:00:10Z");
    assert!(registry.remove(agent_id).await.is_some());
    assert!(registry.get(agent_id).await.is_none());
}

#[tokio::test]
async fn sessions_token_scoped_remove_preserves_replacement() {
    let registry = SessionRegistry::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let old_token = SessionToken::new();
    let new_token = SessionToken::new();
    let (old_wake_sender, _) = mpsc::channel(1);
    let (old_close_sender, _) = mpsc::channel(1);
    let (new_wake_sender, _) = mpsc::channel(1);
    let (new_close_sender, _) = mpsc::channel(1);

    registry
        .register(AgentSession {
            token: old_token,
            tenant_id,
            agent_id,
            name: "agent".to_string(),
            version: "0.1.0".to_string(),
            connected_at: "2026-06-20T00:00:00Z".to_string(),
            last_heartbeat_at: "2026-06-20T00:00:00Z".to_string(),
            wake_sender: old_wake_sender,
            close_sender: old_close_sender,
            command_sender: mpsc::channel(1).0,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;
    registry
        .register(AgentSession {
            token: new_token,
            tenant_id,
            agent_id,
            name: "agent".to_string(),
            version: "0.1.0".to_string(),
            connected_at: "2026-06-20T00:00:10Z".to_string(),
            last_heartbeat_at: "2026-06-20T00:00:10Z".to_string(),
            wake_sender: new_wake_sender,
            close_sender: new_close_sender,
            command_sender: mpsc::channel(1).0,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;

    assert!(
        registry
            .remove_if_current(agent_id, old_token)
            .await
            .is_none()
    );
    assert_eq!(registry.get(agent_id).await.unwrap().token, new_token);
}

#[tokio::test]
async fn sessions_close_local_agent_removes_matching_session_only() {
    let registry = SessionRegistry::new();
    let tenant_id = TenantId::new();
    let other_tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let (wake_sender, _) = mpsc::channel(1);
    let (close_sender, mut close_receiver) = mpsc::channel(1);

    registry
        .register(AgentSession {
            token: SessionToken::new(),
            tenant_id,
            agent_id,
            name: "agent".to_string(),
            version: "0.1.0".to_string(),
            connected_at: "2026-06-20T00:00:00Z".to_string(),
            last_heartbeat_at: "2026-06-20T00:00:00Z".to_string(),
            wake_sender,
            close_sender,
            command_sender: mpsc::channel(1).0,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;

    assert!(
        registry
            .close_local_agent(other_tenant_id, agent_id)
            .await
            .is_none()
    );
    assert!(registry.get(agent_id).await.is_some());

    assert!(
        registry
            .close_local_agent(tenant_id, agent_id)
            .await
            .is_some()
    );
    assert!(registry.get(agent_id).await.is_none());
    tokio::time::timeout(Duration::from_secs(1), close_receiver.recv())
        .await
        .expect("agent session should receive close")
        .expect("close channel should stay open");
}

#[tokio::test]
async fn sessions_close_local_agent_is_not_blocked_by_in_flight_current_operation() {
    let registry = SessionRegistry::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let token = SessionToken::new();
    let (wake_sender, _) = mpsc::channel(1);
    let (close_sender, mut close_receiver) = mpsc::channel(1);
    let (operation_started, operation_started_receiver) = tokio::sync::oneshot::channel();
    let (finish_operation, finish_operation_receiver) = tokio::sync::oneshot::channel();

    registry
        .register(AgentSession {
            token,
            tenant_id,
            agent_id,
            name: "agent".to_string(),
            version: "0.1.0".to_string(),
            connected_at: "2026-06-20T00:00:00Z".to_string(),
            last_heartbeat_at: "2026-06-20T00:00:00Z".to_string(),
            wake_sender,
            close_sender,
            command_sender: mpsc::channel(1).0,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;

    let operation_registry = registry.clone();
    let operation = tokio::spawn(async move {
        operation_registry
            .while_current(agent_id, token, || async move {
                let _ = operation_started.send(());
                let _ = finish_operation_receiver.await;
                1
            })
            .await
    });
    operation_started_receiver.await.unwrap();

    assert!(
        registry
            .close_local_agent(tenant_id, agent_id)
            .await
            .is_some()
    );

    assert!(registry.get(agent_id).await.is_none());
    tokio::time::timeout(Duration::from_secs(1), close_receiver.recv())
        .await
        .expect("agent session should receive close")
        .expect("close channel should stay open");
    let _ = finish_operation.send(());
    assert_eq!(operation.await.unwrap(), None);
}

#[tokio::test]
async fn sessions_live_dispatch_rechecks_token_before_send() {
    let registry = SessionRegistry::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let old_token = SessionToken::new();
    let new_token = SessionToken::new();
    let (old_command_sender, mut old_command_receiver) = mpsc::channel(1);
    let (new_command_sender, _new_command_receiver) = mpsc::channel(1);

    registry
        .register(test_session(
            tenant_id,
            agent_id,
            old_token,
            old_command_sender,
        ))
        .await;
    registry
        .register(test_session(
            tenant_id,
            agent_id,
            new_token,
            new_command_sender,
        ))
        .await;

    let err = registry
        .try_dispatch_live_command(
            tenant_id,
            agent_id,
            old_token,
            CommandId::new(),
            test_command(CommandId::new()),
        )
        .await
        .unwrap_err();

    assert_eq!(err, LiveDispatchError::NotCurrent);
    assert!(old_command_receiver.try_recv().is_err());
}

#[tokio::test]
async fn sessions_pending_live_command_ids_aggregates_all_sessions() {
    let registry = SessionRegistry::new();
    let tenant_id = TenantId::new();
    let agent_a = AgentId::new();
    let agent_b = AgentId::new();
    let token_a = SessionToken::new();
    let token_b = SessionToken::new();
    let (sender_a, _receiver_a) = mpsc::channel(2);
    let (sender_b, _receiver_b) = mpsc::channel(2);
    let command_a = CommandId::new();
    let command_b = CommandId::new();

    registry
        .register(test_session(tenant_id, agent_a, token_a, sender_a))
        .await;
    registry
        .register(test_session(tenant_id, agent_b, token_b, sender_b))
        .await;

    registry
        .try_dispatch_live_command(
            tenant_id,
            agent_a,
            token_a,
            command_a,
            test_command(command_a),
        )
        .await
        .unwrap();
    registry
        .try_dispatch_live_command(
            tenant_id,
            agent_b,
            token_b,
            command_b,
            test_command(command_b),
        )
        .await
        .unwrap();

    let pending = registry.pending_live_command_ids().await;

    assert!(pending.contains(&command_a));
    assert!(pending.contains(&command_b));
}

#[tokio::test]
async fn sessions_replacement_race_does_not_leave_pending_command() {
    let registry = SessionRegistry::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let old_token = SessionToken::new();
    let new_token = SessionToken::new();
    let (old_command_sender, mut old_command_receiver) = mpsc::channel(1);
    let (new_command_sender, _new_command_receiver) = mpsc::channel(1);
    let command_id = CommandId::new();

    registry
        .register(test_session(
            tenant_id,
            agent_id,
            old_token,
            old_command_sender,
        ))
        .await;
    registry
        .register(test_session(
            tenant_id,
            agent_id,
            new_token,
            new_command_sender,
        ))
        .await;

    let err = registry
        .try_dispatch_live_command(
            tenant_id,
            agent_id,
            old_token,
            command_id,
            HubCommand {
                command_id: command_id.to_string(),
                command: Some(hub_command::Command::LinkPrinter(LinkPrinter {
                    host: "192.0.2.10".to_owned(),
                    access_code: "SECRET-LINK-CODE".to_owned(),
                    name: String::new(),
                    printer_type: "BambuLab".to_owned(),
                })),
            },
        )
        .await
        .unwrap_err();

    assert_eq!(err, LiveDispatchError::NotCurrent);
    assert!(old_command_receiver.try_recv().is_err());
    assert!(
        !registry
            .pending_live_command_ids()
            .await
            .contains(&command_id)
    );
}

#[tokio::test]
async fn sessions_wake_local_agent_wakes_matching_online_agent() {
    let state = AppState::sqlite_for_tests().await.unwrap();
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let (wake_sender, mut wake_receiver) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);

    state
        .sessions()
        .register(AgentSession {
            token: SessionToken::new(),
            tenant_id: tenant.id,
            agent_id: agent.id,
            name: agent.name,
            version: "0.1.0".to_string(),
            connected_at: "2026-06-20T00:00:00Z".to_string(),
            last_heartbeat_at: "2026-06-20T00:00:00Z".to_string(),
            wake_sender,
            close_sender,
            command_sender: mpsc::channel(1).0,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;

    let command = state
        .commands()
        .enqueue_refresh_printers(tenant.id, agent.id)
        .await
        .unwrap();
    state.sessions().wake_local_agent(tenant.id, agent.id).await;

    assert_eq!(command.tenant_id, tenant.id);
    assert!(wake_receiver.recv().await.is_some());
}

#[tokio::test]
async fn sessions_expire_stale_marks_agent_offline() {
    let state = AppState::sqlite_for_tests().await.unwrap();
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let token = SessionToken::new();
    state
        .agents()
        .claim_online_session(
            tenant.id,
            agent.id,
            &token.persisted_id(),
            "0.1.0",
            "2026-06-20T00:00:00Z",
        )
        .await
        .unwrap();
    let (wake_sender, _) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);

    state
        .sessions()
        .register(AgentSession {
            token,
            tenant_id: tenant.id,
            agent_id: agent.id,
            name: agent.name,
            version: "0.1.0".to_string(),
            connected_at: "2026-06-20T00:00:00Z".to_string(),
            last_heartbeat_at: "2026-06-20T00:00:00Z".to_string(),
            wake_sender,
            close_sender,
            command_sender: mpsc::channel(1).0,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;

    let expired = state
        .sessions()
        .expire_stale(
            "2026-06-20T00:01:00Z",
            Duration::from_secs(45),
            state.agents(),
        )
        .await
        .unwrap();

    assert_eq!(expired.len(), 1);
    assert!(state.sessions().get(agent.id).await.is_none());
    let persisted = state.agents().get(agent.id).await.unwrap().unwrap();
    assert_eq!(persisted.status, AgentStatus::Offline);
}
