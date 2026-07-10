use super::*;
use pandar_core::CommandStatus;

#[tokio::test]
async fn claim_returns_before_waiting_for_wrong_or_nonpending_commands() {
    let registry = SessionRegistry::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let token = SessionToken::new();
    let session = capable_session(tenant_id, agent_id, token, mpsc::channel(1).0);
    let transition = session.live_command_transition.clone();
    registry.register(session).await;
    let _held = transition.lock_owned().await;

    let wrong_token = tokio::time::timeout(
        Duration::from_millis(50),
        registry.claim_current_live_command(
            tenant_id,
            agent_id,
            SessionToken::new(),
            CommandId::new(),
        ),
    )
    .await
    .expect("wrong token must not wait for the transition permit");
    assert!(matches!(wrong_token, LiveCommandClaimOutcome::NotCurrent));

    let not_pending = tokio::time::timeout(
        Duration::from_millis(50),
        registry.claim_current_live_command(tenant_id, agent_id, token, CommandId::new()),
    )
    .await
    .expect("nonpending command must not wait for the transition permit");
    assert!(matches!(not_pending, LiveCommandClaimOutcome::NotPending));
}

#[tokio::test]
async fn claim_preserves_link_secret_and_removes_pending_without_registry_reentry() {
    let registry = SessionRegistry::new();
    let tenant_id = TenantId::new();
    let agent_id = AgentId::new();
    let token = SessionToken::new();
    let command_id = CommandId::new();
    let (command_sender, _command_receiver) = mpsc::channel(1);
    registry
        .register(session_with_capabilities(
            tenant_id,
            agent_id,
            token,
            command_sender,
            [],
        ))
        .await;
    registry
        .try_dispatch_live_command(
            tenant_id,
            agent_id,
            token,
            command_id,
            link_command(command_id, "SECRET-LINK-CODE"),
        )
        .await
        .unwrap();

    let LiveCommandClaimOutcome::Claim(claim) = registry
        .claim_current_live_command(tenant_id, agent_id, token, command_id)
        .await
    else {
        panic!("expected live command claim");
    };
    assert_eq!(claim.access_code(), Some("SECRET-LINK-CODE"));
    claim.remove_pending();
    drop(claim);

    assert!(
        !registry
            .pending_live_command_ids()
            .await
            .contains(&command_id)
    );
}

#[tokio::test]
async fn claim_that_wins_before_replacement_removes_itself_before_cleanup() {
    let (state, tenant_id, agent_id, command) = live_link_fixture("claim-wins").await;
    let old_token = SessionToken::new();
    let (old_command_sender, _old_command_receiver) = mpsc::channel(1);
    state
        .sessions()
        .register(session_with_capabilities(
            tenant_id,
            agent_id,
            old_token,
            old_command_sender,
            [],
        ))
        .await;
    state
        .sessions()
        .try_dispatch_live_command(
            tenant_id,
            agent_id,
            old_token,
            command.id,
            link_command(command.id, "SECRET-LINK-CODE"),
        )
        .await
        .unwrap();
    let LiveCommandClaimOutcome::Claim(claim) = state
        .sessions()
        .claim_current_live_command(tenant_id, agent_id, old_token, command.id)
        .await
    else {
        panic!("expected live command claim");
    };

    let new_token = SessionToken::new();
    let removed = state
        .sessions()
        .register(session_with_capabilities(
            tenant_id,
            agent_id,
            new_token,
            mpsc::channel(1).0,
            [],
        ))
        .await
        .unwrap();
    let cleanup_state = state.clone();
    let mut cleanup = tokio::spawn(async move {
        fail_pending_live_commands(
            &cleanup_state,
            tenant_id,
            agent_id,
            removed,
            "replacement cleanup",
        )
        .await;
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(25), &mut cleanup)
            .await
            .is_err(),
        "cleanup must wait for the winning transition claim"
    );

    claim.remove_pending();
    drop(claim);
    tokio::time::timeout(Duration::from_secs(1), cleanup)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        state
            .commands()
            .get_for_tenant(tenant_id, command.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        CommandStatus::Sent
    );
    assert_eq!(
        state.sessions().get(agent_id).await.unwrap().token,
        new_token
    );
}

#[tokio::test]
async fn replacement_that_wins_first_cleans_old_pending_and_makes_claim_not_current() {
    let (state, tenant_id, agent_id, command) = live_link_fixture("replacement-wins").await;
    let old_token = SessionToken::new();
    let (old_command_sender, _old_command_receiver) = mpsc::channel(1);
    state
        .sessions()
        .register(session_with_capabilities(
            tenant_id,
            agent_id,
            old_token,
            old_command_sender,
            [],
        ))
        .await;
    state
        .sessions()
        .try_dispatch_live_command(
            tenant_id,
            agent_id,
            old_token,
            command.id,
            link_command(command.id, "SECRET-LINK-CODE"),
        )
        .await
        .unwrap();

    let new_token = SessionToken::new();
    let removed = state
        .sessions()
        .register(session_with_capabilities(
            tenant_id,
            agent_id,
            new_token,
            mpsc::channel(1).0,
            [],
        ))
        .await
        .unwrap();
    fail_pending_live_commands(&state, tenant_id, agent_id, removed, "replacement cleanup").await;

    assert!(matches!(
        state
            .sessions()
            .claim_current_live_command(tenant_id, agent_id, old_token, command.id)
            .await,
        LiveCommandClaimOutcome::NotCurrent
    ));
    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, CommandStatus::Failed);
    assert_eq!(stored.error.as_deref(), Some("replacement cleanup"));
    assert_eq!(
        state.sessions().get(agent_id).await.unwrap().token,
        new_token
    );
}
