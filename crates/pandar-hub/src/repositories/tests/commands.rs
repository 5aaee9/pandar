use pandar_core::{AgentId, CommandId, CommandStatus};

use super::*;

#[tokio::test]
async fn command_enqueue_rejects_missing_agent() {
    let (_, tenants, _, _, commands) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();

    let err = commands
        .enqueue_refresh_printers(tenant.id, AgentId::new())
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::MissingAgent));
}

#[tokio::test]
async fn command_enqueue_rejects_wrong_tenant() {
    let (_, tenants, agents, _, commands) = repositories().await;
    let acme = tenants.create("acme", "Acme Labs").await.unwrap();
    let beta = tenants.create("beta", "Beta Labs").await.unwrap();
    let agent = agents.create(acme.id, "agent").await.unwrap();

    let err = commands
        .enqueue_refresh_printers(beta.id, agent.id)
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::CommandOwnershipMismatch));
}

#[tokio::test]
async fn command_queue_filters_by_tenant_and_agent() {
    let (_, tenants, agents, _, commands) = repositories().await;
    let acme = tenants.create("acme", "Acme Labs").await.unwrap();
    let beta = tenants.create("beta", "Beta Labs").await.unwrap();
    let acme_agent = agents.create(acme.id, "agent").await.unwrap();
    let other_acme_agent = agents.create(acme.id, "other").await.unwrap();
    let beta_agent = agents.create(beta.id, "agent").await.unwrap();

    let expected = commands
        .enqueue_refresh_printers(acme.id, acme_agent.id)
        .await
        .unwrap();
    commands
        .enqueue_refresh_printers(acme.id, other_acme_agent.id)
        .await
        .unwrap();
    commands
        .enqueue_refresh_printers(beta.id, beta_agent.id)
        .await
        .unwrap();

    assert_eq!(
        commands
            .next_queued_for_agent(acme.id, acme_agent.id)
            .await
            .unwrap()
            .unwrap()
            .id,
        expected.id
    );
}

#[tokio::test]
async fn command_update_rejects_missing_command() {
    let (_, _, commands, tenant, agent) = command_repositories().await;

    let err = commands
        .mark_sent(CommandId::new(), tenant.id, agent.id)
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::MissingCommand));
}

#[tokio::test]
async fn command_update_rejects_wrong_tenant() {
    let (tenants, _, commands, tenant, agent) = command_repositories().await;
    let other = tenants.create("beta", "Beta Labs").await.unwrap();
    let command = commands
        .enqueue_refresh_printers(tenant.id, agent.id)
        .await
        .unwrap();

    let err = commands
        .mark_sent(command.id, other.id, agent.id)
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::CommandOwnershipMismatch));
}

#[tokio::test]
async fn command_update_rejects_wrong_agent() {
    let (_, agents, commands, tenant, agent) = command_repositories().await;
    let other = agents.create(tenant.id, "other").await.unwrap();
    let command = commands
        .enqueue_refresh_printers(tenant.id, agent.id)
        .await
        .unwrap();

    let err = commands
        .mark_sent(command.id, tenant.id, other.id)
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::CommandOwnershipMismatch));
}

#[tokio::test]
async fn command_sent_ack_success_flow() {
    let (_, _, commands, tenant, agent) = command_repositories().await;
    let command = commands
        .enqueue_refresh_printers(tenant.id, agent.id)
        .await
        .unwrap();

    let sent = commands
        .mark_sent(command.id, tenant.id, agent.id)
        .await
        .unwrap();
    assert_eq!(sent.status, CommandStatus::Sent);
    let acked = commands
        .mark_acknowledged(command.id, tenant.id, agent.id)
        .await
        .unwrap();
    assert_eq!(acked.status, CommandStatus::Acknowledged);
    let succeeded = commands
        .mark_succeeded(command.id, tenant.id, agent.id)
        .await
        .unwrap();
    assert_eq!(succeeded.status, CommandStatus::Succeeded);
}

#[tokio::test]
async fn command_ack_failure_marks_failed() {
    let (_, _, commands, tenant, agent) = command_repositories().await;
    let command_id = enqueue_sent(&commands, tenant.id, agent.id).await;

    let failed = commands
        .mark_failed(command_id, tenant.id, agent.id, "rejected")
        .await
        .unwrap();

    assert_eq!(failed.status, CommandStatus::Failed);
    assert_eq!(failed.error.as_deref(), Some("rejected"));
}

#[tokio::test]
async fn command_result_failure_marks_failed() {
    let (_, _, commands, tenant, agent) = command_repositories().await;
    let command_id = enqueue_sent(&commands, tenant.id, agent.id).await;
    commands
        .mark_acknowledged(command_id, tenant.id, agent.id)
        .await
        .unwrap();

    let failed = commands
        .mark_failed(command_id, tenant.id, agent.id, "printer unavailable")
        .await
        .unwrap();

    assert_eq!(failed.status, CommandStatus::Failed);
    assert_eq!(failed.error.as_deref(), Some("printer unavailable"));
}

#[tokio::test]
async fn command_duplicate_terminal_events_are_idempotent() {
    let (_, _, commands, tenant, agent) = command_repositories().await;
    let success_id = enqueue_sent(&commands, tenant.id, agent.id).await;
    let first_success = commands
        .mark_succeeded(success_id, tenant.id, agent.id)
        .await
        .unwrap();
    assert_eq!(
        commands
            .mark_succeeded(success_id, tenant.id, agent.id)
            .await
            .unwrap(),
        first_success
    );

    let failure_id = enqueue_sent(&commands, tenant.id, agent.id).await;
    let first_failure = commands
        .mark_failed(failure_id, tenant.id, agent.id, "first")
        .await
        .unwrap();
    let duplicate = commands
        .mark_failed(failure_id, tenant.id, agent.id, "second")
        .await
        .unwrap();
    assert_eq!(duplicate.error.as_deref(), Some("first"));
    assert_eq!(duplicate, first_failure);
}

#[tokio::test]
async fn command_stale_events_are_rejected() {
    let (_, _, commands, tenant, agent) = command_repositories().await;
    let queued = commands
        .enqueue_refresh_printers(tenant.id, agent.id)
        .await
        .unwrap();
    let err = commands
        .mark_acknowledged(queued.id, tenant.id, agent.id)
        .await
        .unwrap_err();
    assert!(
        matches!(err, RepositoryError::InvalidCommandTransition { from, action } if from == "queued" && action == "acknowledge")
    );

    let acked_id = enqueue_sent(&commands, tenant.id, agent.id).await;
    commands
        .mark_acknowledged(acked_id, tenant.id, agent.id)
        .await
        .unwrap();
    let err = commands
        .mark_acknowledged(acked_id, tenant.id, agent.id)
        .await
        .unwrap_err();
    assert!(
        matches!(err, RepositoryError::InvalidCommandTransition { from, action } if from == "acknowledged" && action == "acknowledge")
    );

    let failed_id = enqueue_sent(&commands, tenant.id, agent.id).await;
    commands
        .mark_failed(failed_id, tenant.id, agent.id, "first")
        .await
        .unwrap();
    let err = commands
        .mark_succeeded(failed_id, tenant.id, agent.id)
        .await
        .unwrap_err();
    assert!(
        matches!(err, RepositoryError::InvalidCommandTransition { from, action } if from == "failed" && action == "succeed")
    );
}
