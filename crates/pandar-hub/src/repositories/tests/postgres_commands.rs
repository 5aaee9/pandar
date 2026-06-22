use pandar_core::{AgentId, AgentStatus, CommandId, CommandStatus};

use super::*;
use crate::repositories::tests::postgres::postgres_database;

#[tokio::test]
async fn postgres_command_repository_behavior_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database);
    let acme = tenants.create("acme", "Acme Labs").await.unwrap();
    let beta = tenants.create("beta", "Beta Labs").await.unwrap();
    let agent = agents.create(acme.id, "agent").await.unwrap();
    let other_agent = agents.create(acme.id, "other").await.unwrap();
    let beta_agent = agents.create(beta.id, "agent").await.unwrap();

    assert_eq!(agents.get(agent.id).await.unwrap(), Some(agent.clone()));
    assert_eq!(
        agents
            .update_connection(
                agent.id,
                AgentStatus::Online,
                Some("0.2.0"),
                "2026-06-20T01:00:00Z"
            )
            .await
            .unwrap()
            .status,
        AgentStatus::Online
    );
    assert_eq!(
        agents
            .mark_offline(agent.id, "2026-06-20T01:01:00Z")
            .await
            .unwrap()
            .status,
        AgentStatus::Offline
    );

    assert!(matches!(
        commands
            .enqueue_refresh_printers(acme.id, AgentId::new())
            .await
            .unwrap_err(),
        RepositoryError::MissingAgent
    ));
    assert!(matches!(
        commands
            .enqueue_refresh_printers(beta.id, agent.id)
            .await
            .unwrap_err(),
        RepositoryError::CommandOwnershipMismatch
    ));

    let command = commands
        .enqueue_refresh_printers(acme.id, agent.id)
        .await
        .unwrap();
    commands
        .enqueue_refresh_printers(acme.id, other_agent.id)
        .await
        .unwrap();
    commands
        .enqueue_refresh_printers(beta.id, beta_agent.id)
        .await
        .unwrap();
    assert_eq!(
        commands
            .next_queued_for_agent(acme.id, agent.id)
            .await
            .unwrap()
            .unwrap()
            .id,
        command.id
    );
    assert!(matches!(
        commands
            .mark_sent(CommandId::new(), acme.id, agent.id)
            .await
            .unwrap_err(),
        RepositoryError::MissingCommand
    ));
    assert!(matches!(
        commands
            .mark_sent(command.id, beta.id, agent.id)
            .await
            .unwrap_err(),
        RepositoryError::CommandOwnershipMismatch
    ));
    assert!(matches!(
        commands
            .mark_sent(command.id, acme.id, other_agent.id)
            .await
            .unwrap_err(),
        RepositoryError::CommandOwnershipMismatch
    ));

    assert_eq!(
        commands
            .mark_sent(command.id, acme.id, agent.id)
            .await
            .unwrap()
            .status,
        CommandStatus::Sent
    );
    assert_eq!(
        commands
            .mark_acknowledged(command.id, acme.id, agent.id)
            .await
            .unwrap()
            .status,
        CommandStatus::Acknowledged
    );
    assert_eq!(
        commands
            .mark_succeeded(command.id, acme.id, agent.id)
            .await
            .unwrap()
            .status,
        CommandStatus::Succeeded
    );
    assert_eq!(
        commands
            .mark_succeeded(command.id, acme.id, agent.id)
            .await
            .unwrap()
            .status,
        CommandStatus::Succeeded
    );

    let failed = enqueue_sent(&commands, acme.id, agent.id).await;
    let first_failure = commands
        .mark_failed(failed, acme.id, agent.id, "first")
        .await
        .unwrap();
    assert_eq!(
        commands
            .mark_failed(failed, acme.id, agent.id, "second")
            .await
            .unwrap()
            .error,
        first_failure.error
    );
    assert!(matches!(
        commands
            .mark_acknowledged(failed, acme.id, agent.id)
            .await
            .unwrap_err(),
        RepositoryError::InvalidCommandTransition { .. }
    ));

    let ack_failed = enqueue_sent(&commands, acme.id, agent.id).await;
    commands
        .mark_acknowledged(ack_failed, acme.id, agent.id)
        .await
        .unwrap();
    let result_failure = commands
        .mark_failed(ack_failed, acme.id, agent.id, "printer unavailable")
        .await
        .unwrap();
    assert_eq!(result_failure.status, CommandStatus::Failed);
    assert_eq!(result_failure.error.as_deref(), Some("printer unavailable"));

    let diagnostic_id = enqueue_sent(&commands, acme.id, agent.id).await;
    commands
        .mark_acknowledged(diagnostic_id, acme.id, agent.id)
        .await
        .unwrap();
    let diagnostic_result = r#"{"type":"printer_diagnostic","overall":"problem"}"#;
    let diagnostic_success = commands
        .mark_succeeded_with_result(
            diagnostic_id,
            acme.id,
            agent.id,
            Some(diagnostic_result.to_owned()),
        )
        .await
        .unwrap();
    assert_eq!(diagnostic_success.status, CommandStatus::Succeeded);
    assert_eq!(
        diagnostic_success.result_json.as_deref(),
        Some(diagnostic_result)
    );

    let unexpected_id = enqueue_sent(&commands, acme.id, agent.id).await;
    let unexpected_result = r#"{"type":"printer_diagnostic","checks":[]}"#;
    let unexpected_failure = commands
        .mark_failed_with_result(
            unexpected_id,
            acme.id,
            agent.id,
            "unexpected diagnostics failure",
            Some(unexpected_result.to_owned()),
        )
        .await
        .unwrap();
    assert_eq!(unexpected_failure.status, CommandStatus::Failed);
    assert_eq!(
        unexpected_failure.result_json.as_deref(),
        Some(unexpected_result)
    );

    assert_eq!(
        commands
            .get_for_tenant(acme.id, diagnostic_success.id)
            .await
            .unwrap()
            .unwrap()
            .result_json
            .as_deref(),
        Some(diagnostic_result)
    );
    assert_eq!(
        commands
            .get_for_tenant(beta.id, diagnostic_success.id)
            .await
            .unwrap(),
        None
    );
}
