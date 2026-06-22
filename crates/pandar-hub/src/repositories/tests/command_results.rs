use pandar_core::CommandStatus;

use super::*;

#[tokio::test]
async fn command_result_json_persists_on_succeeded_diagnostics() {
    let (_, _, commands, tenant, agent) = command_repositories().await;
    let command_id = enqueue_sent(&commands, tenant.id, agent.id).await;
    commands
        .mark_acknowledged(command_id, tenant.id, agent.id)
        .await
        .unwrap();
    let result_json = r#"{"type":"printer_diagnostic","overall":"problem"}"#;

    let succeeded = commands
        .mark_succeeded_with_result(
            command_id,
            tenant.id,
            agent.id,
            Some(result_json.to_owned()),
        )
        .await
        .unwrap();

    assert_eq!(succeeded.status, CommandStatus::Succeeded);
    assert_eq!(succeeded.result_json.as_deref(), Some(result_json));
}

#[tokio::test]
async fn command_result_json_persists_on_failed_unexpected_results() {
    let (_, _, commands, tenant, agent) = command_repositories().await;
    let command_id = enqueue_sent(&commands, tenant.id, agent.id).await;
    let result_json = r#"{"type":"printer_diagnostic","checks":[]}"#;

    let failed = commands
        .mark_failed_with_result(
            command_id,
            tenant.id,
            agent.id,
            "unexpected diagnostics failure",
            Some(result_json.to_owned()),
        )
        .await
        .unwrap();

    assert_eq!(failed.status, CommandStatus::Failed);
    assert_eq!(
        failed.error.as_deref(),
        Some("unexpected diagnostics failure")
    );
    assert_eq!(failed.result_json.as_deref(), Some(result_json));
}

#[tokio::test]
async fn command_get_for_tenant_is_tenant_scoped() {
    let (tenants, agents, commands, tenant, agent) = command_repositories().await;
    let other_tenant = tenants.create("beta", "Beta Labs").await.unwrap();
    agents.create(other_tenant.id, "other").await.unwrap();
    let command = commands
        .enqueue_refresh_printers(tenant.id, agent.id)
        .await
        .unwrap();

    assert_eq!(
        commands
            .get_for_tenant(tenant.id, command.id)
            .await
            .unwrap()
            .unwrap()
            .id,
        command.id
    );
    assert_eq!(
        commands
            .get_for_tenant(other_tenant.id, command.id)
            .await
            .unwrap(),
        None
    );
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
