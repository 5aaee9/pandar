use super::*;

#[tokio::test]
async fn delayed_self_close_delivery_preserves_replacement_session() {
    let state = AppState::sqlite_for_tests().await.unwrap();
    let (_control_plane, ready) = spawn_control_plane_ready(state.clone());
    ready.await.unwrap().unwrap();
    let tenant = state
        .tenants()
        .create("self-close-race", "Self Close Race")
        .await
        .unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let old = register_session(&state, tenant.id, agent.id).await;
    let transition = old.transition.clone().lock_owned().await;

    let close_state = state.clone();
    let close = tokio::spawn(async move {
        close_state.close_agent(tenant.id, agent.id).await;
    });
    wait_for_close(old.close_receiver).await;
    assert!(state.sessions().get(agent.id).await.is_none());

    let mut replacement = register_session(&state, tenant.id, agent.id).await;
    drop(transition);
    close.await.unwrap();
    wait_for_received_control_message(&state).await;

    assert_eq!(
        state.sessions().get(agent.id).await.unwrap().token,
        replacement.token
    );
    assert!(replacement.close_receiver.try_recv().is_err());
}

struct RegisteredSession {
    token: SessionToken,
    transition: Arc<tokio::sync::Mutex<()>>,
    close_receiver: mpsc::Receiver<()>,
}

async fn register_session(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> RegisteredSession {
    let token = SessionToken::new();
    let transition = Arc::new(tokio::sync::Mutex::new(()));
    let (close_sender, close_receiver) = mpsc::channel(1);
    let replaced = state
        .sessions()
        .register(AgentSession {
            token,
            tenant_id,
            agent_id,
            name: "agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender: mpsc::channel(1).0,
            close_sender,
            command_sender: mpsc::channel(1).0,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
            live_command_transition: transition.clone(),
        })
        .await;
    assert!(replaced.is_none());
    RegisteredSession {
        token,
        transition,
        close_receiver,
    }
}

async fn wait_for_close(mut close_receiver: mpsc::Receiver<()>) {
    tokio::time::timeout(Duration::from_secs(1), close_receiver.recv())
        .await
        .unwrap()
        .expect("detached session should receive close");
}

async fn wait_for_received_control_message(state: &AppState) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let received = state
                .metrics()
                .control_plane_snapshot()
                .into_iter()
                .find_map(|(name, count)| (name == "receive_ok").then_some(count))
                .unwrap();
            if received == 1 {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
