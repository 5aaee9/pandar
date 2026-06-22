use pandar_core::{AgentId, AgentStatus, CommandId, CommandStatus, JobStatus};

use super::*;
use crate::repositories::{
    CreatePrintJob,
    test_helpers::{insert_command_fixture, insert_printer_fixture},
};

pub(super) async fn postgres_database() -> Option<Database> {
    let url = match std::env::var("PANDAR_TEST_POSTGRES_URL") {
        Ok(url) => url,
        Err(_) => return None,
    };
    let config = DatabaseConfig::from_url(url).unwrap();
    let database = Database::connect(&config).await.unwrap();
    database.migrate().await.unwrap();
    clear_postgres(&database).await;
    Some(database)
}

pub(super) async fn clear_postgres(database: &Database) {
    let Database::Postgres(pool) = database else {
        panic!("expected PostgreSQL database");
    };
    sqlx::query(
        "TRUNCATE audit_events, api_tokens, jobs, job_artifacts, commands, printers, agents, users, tenants",
    )
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn postgres_core_repository_behavior_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());

    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    insert_command_fixture(&database, tenant.id, agent.id, Some(&printer_id))
        .await
        .unwrap();

    assert_eq!(tenants.list().await.unwrap(), vec![tenant.clone()]);
    assert_eq!(tenants.count().await.unwrap(), 1);
    assert_eq!(
        agents.list_for_tenant(tenant.id).await.unwrap(),
        vec![agent]
    );
    assert!(matches!(
        tenants.create("acme", "Acme Again").await.unwrap_err(),
        RepositoryError::DuplicateTenantSlug
    ));
    assert_eq!(printers.count().await.unwrap(), 1);
    assert_eq!(commands.count().await.unwrap(), 1);
}

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
}

#[tokio::test]
async fn postgres_job_repository_behavior_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();

    let created = jobs
        .create_print_job(CreatePrintJob {
            tenant_id: tenant.id,
            printer_id: printer_id.clone(),
            agent_id: agent.id,
            artifact_id: "artifact-1".to_string(),
            artifact_filename: "plate.3mf".to_string(),
            artifact_content_type: "model/3mf".to_string(),
            artifact_size_bytes: 42,
            artifact_storage_path: format!("{}/artifact-1/plate.3mf", tenant.id),
            plate_id: 1,
            use_ams: true,
            flow_cali: false,
            timelapse: false,
        })
        .await
        .unwrap();

    assert_eq!(jobs.list_for_tenant(tenant.id).await.unwrap().len(), 1);
    assert_eq!(
        jobs.get_for_tenant(tenant.id, created.job.id)
            .await
            .unwrap()
            .unwrap()
            .job
            .id,
        created.job.id
    );
    assert_eq!(
        jobs.mark_for_command(created.job.command_id, JobStatus::Acknowledged, None)
            .await
            .unwrap()
            .unwrap()
            .status,
        JobStatus::Acknowledged
    );

    let err = jobs
        .create_print_job(CreatePrintJob {
            tenant_id: tenant.id,
            printer_id,
            agent_id: agent.id,
            artifact_id: String::new(),
            artifact_filename: "plate.3mf".to_string(),
            artifact_content_type: "model/3mf".to_string(),
            artifact_size_bytes: 42,
            artifact_storage_path: format!("{}/bad/plate.3mf", tenant.id),
            plate_id: 1,
            use_ams: false,
            flow_cali: false,
            timelapse: false,
        })
        .await
        .unwrap_err();
    assert!(matches!(err, RepositoryError::Database(_)));
    assert_eq!(commands.count().await.unwrap(), 1);

    let Database::Postgres(pool) = &database else {
        panic!("expected PostgreSQL database");
    };
    sqlx::query("UPDATE jobs SET status = 'printing' WHERE id = $1")
        .bind(created.job.id.to_string())
        .execute(pool)
        .await
        .unwrap();
    let err = jobs.list_for_tenant(tenant.id).await.unwrap_err();
    assert!(
        matches!(err, RepositoryError::InvalidPersistedJobStatus(status) if status == "printing")
    );
}

#[tokio::test]
async fn postgres_printer_repository_upsert_list_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database);
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();

    let created = printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            PrinterSnapshotUpsert {
                serial_number: "SN-001".to_string(),
                name: "Garage A1".to_string(),
                model: Some("A1 Mini".to_string()),
                status: "idle".to_string(),
                observed_at: "2026-06-21T00:00:00Z".to_string(),
            },
        )
        .await
        .unwrap();
    let updated = printers
        .upsert_snapshot(
            tenant.id,
            agent.id,
            PrinterSnapshotUpsert {
                serial_number: "SN-001".to_string(),
                name: "Garage A1".to_string(),
                model: Some("A1 Mini".to_string()),
                status: "printing".to_string(),
                observed_at: "2026-06-21T00:05:00Z".to_string(),
            },
        )
        .await
        .unwrap();

    assert_eq!(updated.id, created.id);
    assert_eq!(updated.created_at, created.created_at);
    assert_eq!(updated.status, "printing");
    assert_eq!(updated.last_seen_at, "2026-06-21T00:05:00Z");
    assert_eq!(
        printers.list_for_tenant(tenant.id).await.unwrap(),
        vec![updated]
    );
}
