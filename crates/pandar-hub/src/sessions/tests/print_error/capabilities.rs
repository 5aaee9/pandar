use super::*;

#[tokio::test]
async fn capability_token_requires_a_current_matching_capable_session() {
    let registry = SessionRegistry::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let incapable_token = SessionToken::new();

    assert!(
        registry
            .current_token_for_capability(tenant_id, agent_id, AgentCapability::HandlePrintError,)
            .await
            .is_none()
    );

    registry
        .register(session_with_capabilities(
            tenant_id,
            agent_id,
            incapable_token,
            mpsc::channel(1).0,
            [],
        ))
        .await;
    assert!(
        registry
            .current_token_for_capability(tenant_id, agent_id, AgentCapability::HandlePrintError,)
            .await
            .is_none()
    );

    let capable_token = SessionToken::new();
    registry
        .register(capable_session(
            tenant_id,
            agent_id,
            capable_token,
            mpsc::channel(1).0,
        ))
        .await;

    assert_eq!(
        registry
            .current_token_for_capability(tenant_id, agent_id, AgentCapability::HandlePrintError,)
            .await,
        Some(capable_token)
    );
    assert!(
        registry
            .current_token_for_capability(
                TenantId::new(),
                agent_id,
                AgentCapability::HandlePrintError,
            )
            .await
            .is_none()
    );
}

#[tokio::test]
async fn capability_dispatch_registers_pending_and_sends_only_to_current_capable_session() {
    let registry = SessionRegistry::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let token = SessionToken::new();
    let command_id = CommandId::new();
    let (sender, mut receiver) = mpsc::channel(1);
    registry
        .register(capable_session(tenant_id, agent_id, token, sender))
        .await;

    let command = test_command(command_id);
    registry
        .try_dispatch_live_command_with_capability(
            tenant_id,
            agent_id,
            token,
            AgentCapability::HandlePrintError,
            command_id,
            command.clone(),
        )
        .await
        .unwrap();

    assert_eq!(receiver.recv().await.unwrap().unwrap(), command);
    assert!(
        registry
            .pending_live_command_ids()
            .await
            .contains(&command_id)
    );
}

#[tokio::test]
async fn capability_dispatch_rejects_incapable_and_replaced_tokens_without_sending() {
    let registry = SessionRegistry::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let old_token = SessionToken::new();
    let new_token = SessionToken::new();
    let (old_sender, mut old_receiver) = mpsc::channel(1);
    let (new_sender, mut new_receiver) = mpsc::channel(1);
    registry
        .register(capable_session(tenant_id, agent_id, old_token, old_sender))
        .await;
    registry
        .register(session_with_capabilities(
            tenant_id,
            agent_id,
            new_token,
            new_sender,
            [],
        ))
        .await;

    for token in [old_token, new_token] {
        let command_id = CommandId::new();
        assert_eq!(
            registry
                .try_dispatch_live_command_with_capability(
                    tenant_id,
                    agent_id,
                    token,
                    AgentCapability::HandlePrintError,
                    command_id,
                    test_command(command_id),
                )
                .await,
            Err(LiveDispatchError::NotCurrent)
        );
    }

    assert!(old_receiver.try_recv().is_err());
    assert!(new_receiver.try_recv().is_err());
    assert!(registry.pending_live_command_ids().await.is_empty());
}

#[tokio::test]
async fn failed_capability_dispatch_removes_only_the_new_pending_entry() {
    for channel_full in [false, true] {
        let registry = SessionRegistry::new();
        let tenant_id = TenantId::new();
        let agent_id = AgentId::new();
        let token = SessionToken::new();
        let existing_id = CommandId::new();
        let rejected_id = CommandId::new();
        let (sender, receiver) = mpsc::channel(1);
        if channel_full {
            sender.try_send(Ok(test_command(CommandId::new()))).unwrap();
        } else {
            drop(receiver);
        }
        let session = capable_session(tenant_id, agent_id, token, sender);
        session
            .pending_live_commands
            .lock()
            .unwrap()
            .insert(existing_id, PendingLiveCommand::new(None));
        registry.register(session).await;

        let error = registry
            .try_dispatch_live_command_with_capability(
                tenant_id,
                agent_id,
                token,
                AgentCapability::HandlePrintError,
                rejected_id,
                test_command(rejected_id),
            )
            .await
            .unwrap_err();

        assert_eq!(
            error,
            if channel_full {
                LiveDispatchError::ChannelFull
            } else {
                LiveDispatchError::ChannelClosed
            }
        );
        let pending = registry.pending_live_command_ids().await;
        assert!(pending.contains(&existing_id));
        assert!(!pending.contains(&rejected_id));
    }
}

#[tokio::test]
async fn close_local_agent_returns_the_exact_removed_session() {
    let registry = SessionRegistry::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let token = SessionToken::new();
    let (close_sender, mut close_receiver) = mpsc::channel(1);
    let mut session = capable_session(tenant_id, agent_id, token, mpsc::channel(1).0);
    session.close_sender = close_sender;
    let pending = session.pending_live_commands.clone();
    let transition = session.live_command_transition.clone();
    registry.register(session).await;

    assert!(
        registry
            .close_local_agent(TenantId::new(), agent_id)
            .await
            .is_none()
    );
    let removed = registry
        .close_local_agent(tenant_id, agent_id)
        .await
        .expect("matching close returns removed session");

    assert_eq!(removed.token, token);
    assert!(std::sync::Arc::ptr_eq(
        &removed.pending_live_commands,
        &pending
    ));
    assert!(std::sync::Arc::ptr_eq(
        &removed.live_command_transition,
        &transition
    ));
    assert!(registry.get(agent_id).await.is_none());
    tokio::time::timeout(Duration::from_secs(1), close_receiver.recv())
        .await
        .unwrap()
        .expect("removed session receives close signal");
}
