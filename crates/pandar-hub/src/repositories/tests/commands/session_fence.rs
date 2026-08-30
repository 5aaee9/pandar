use std::time::Duration;

use pandar_core::CommandStatus;

use super::super::*;
use crate::repositories::{
    CurrentSessionCommandAction, current_transaction_pause, transition_current_session_command,
};

#[tokio::test]
async fn stale_agent_session_cannot_dispatch_or_complete_commands() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let tenant = tenants
        .create("session-fence", "Session Fence")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let now = pandar_core::created_at_now();
    agents
        .claim_online_session(tenant.id, agent.id, "session-a", "test", &now)
        .await
        .unwrap();

    let sent = commands
        .enqueue_refresh_printers(tenant.id, agent.id)
        .await
        .unwrap();
    transition_current_session_command(
        &database,
        tenant.id,
        agent.id,
        "session-a",
        sent.id,
        CurrentSessionCommandAction::Send,
    )
    .await
    .unwrap();

    let queued = commands
        .enqueue_refresh_printers(tenant.id, agent.id)
        .await
        .unwrap();
    agents
        .claim_online_session(tenant.id, agent.id, "session-b", "test", &now)
        .await
        .unwrap();

    for (command_id, action) in [
        (sent.id, CurrentSessionCommandAction::Acknowledge),
        (queued.id, CurrentSessionCommandAction::Send),
    ] {
        let error = transition_current_session_command(
            &database,
            tenant.id,
            agent.id,
            "session-a",
            command_id,
            action,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, RepositoryError::AgentSessionNotCurrent));
    }

    assert_eq!(
        commands
            .get_for_tenant(tenant.id, sent.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        CommandStatus::Sent
    );
    assert_eq!(
        commands
            .get_for_tenant(tenant.id, queued.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        CommandStatus::Queued
    );
}

#[tokio::test]
async fn session_replacement_waits_for_in_flight_command_transition() {
    let (database, tenants, agents, _, commands, _) = repositories().await;
    let tenant = tenants
        .create("session-linearization", "Session Linearization")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let now = pandar_core::created_at_now();
    agents
        .claim_online_session(tenant.id, agent.id, "session-a", "test", &now)
        .await
        .unwrap();
    let command = commands
        .enqueue_refresh_printers(tenant.id, agent.id)
        .await
        .unwrap();
    let mut pause = current_transaction_pause::install("session-a");

    let transition_database = database.clone();
    let transition = tokio::spawn(async move {
        transition_current_session_command(
            &transition_database,
            tenant.id,
            agent.id,
            "session-a",
            command.id,
            CurrentSessionCommandAction::Send,
        )
        .await
    });
    pause.wait_until_reached().await;

    let replacement_agents = agents.clone();
    let mut replacement = tokio::spawn(async move {
        replacement_agents
            .claim_online_session(tenant.id, agent.id, "session-b", "test", &now)
            .await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut replacement)
            .await
            .is_err(),
        "session replacement must wait for the fenced command transition"
    );

    pause.resume();
    assert_eq!(
        transition.await.unwrap().unwrap().status,
        CommandStatus::Sent
    );
    replacement.await.unwrap().unwrap();
}
