use super::*;
use crate::repositories::{CurrentSessionCommandAction, transition_current_session_command};

#[tokio::test]
async fn postgres_current_session_fences_command_transitions() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let tenant = tenants
        .create("postgres-session-fence", "PostgreSQL Session Fence")
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

    transition_current_session_command(
        &database,
        tenant.id,
        agent.id,
        "session-a",
        command.id,
        CurrentSessionCommandAction::Send,
    )
    .await
    .unwrap();
    agents
        .claim_online_session(tenant.id, agent.id, "session-b", "test", &now)
        .await
        .unwrap();

    let error = transition_current_session_command(
        &database,
        tenant.id,
        agent.id,
        "session-a",
        command.id,
        CurrentSessionCommandAction::Acknowledge,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, RepositoryError::AgentSessionNotCurrent));
    assert_eq!(
        commands
            .get_for_tenant(tenant.id, command.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        CommandStatus::Sent
    );
}
