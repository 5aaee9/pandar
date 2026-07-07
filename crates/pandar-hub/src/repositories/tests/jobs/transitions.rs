use super::*;

#[tokio::test]
async fn job_repository_mark_for_command_tracks_ack_success_failure() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let created = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap();

    let acked = jobs
        .mark_for_command(created.job.command_id, JobStatus::Acknowledged, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(acked.status, JobStatus::Acknowledged);
    let succeeded = jobs
        .mark_for_command(created.job.command_id, JobStatus::Succeeded, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(succeeded.status, JobStatus::Succeeded);
    let duplicate = jobs
        .mark_for_command(
            created.job.command_id,
            JobStatus::Failed,
            Some("late".to_string()),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(duplicate.status, JobStatus::Succeeded);
    assert_eq!(
        jobs.mark_for_command(
            CommandId::new(),
            JobStatus::Failed,
            Some("missing".to_string())
        )
        .await
        .unwrap(),
        None
    );
}

#[tokio::test]
async fn job_repository_print_command_transitions_update_command_and_job_together() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let created = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap();

    let sent = jobs
        .mark_print_sent(created.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    assert_eq!(sent.status, pandar_core::CommandStatus::Sent);
    assert_eq!(
        jobs.get_for_tenant(tenant.id, created.job.id)
            .await
            .unwrap()
            .unwrap()
            .job
            .status,
        JobStatus::Sent
    );

    let acked = jobs
        .mark_print_acknowledged(created.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    assert_eq!(acked.status, pandar_core::CommandStatus::Acknowledged);
    assert_eq!(
        jobs.get_for_tenant(tenant.id, created.job.id)
            .await
            .unwrap()
            .unwrap()
            .job
            .status,
        JobStatus::Acknowledged
    );

    let succeeded = jobs
        .mark_print_succeeded(created.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    assert_eq!(succeeded.status, pandar_core::CommandStatus::Succeeded);
    assert_eq!(
        jobs.get_for_tenant(tenant.id, created.job.id)
            .await
            .unwrap()
            .unwrap()
            .job
            .status,
        JobStatus::Succeeded
    );
    assert_eq!(commands.count().await.unwrap(), 1);
}

#[tokio::test]
async fn invalid_persisted_job_status_is_reported() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let created = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap();
    let Database::Sqlite(pool) = &database else {
        panic!("expected SQLite database");
    };
    sqlx::query("UPDATE jobs SET status = 'printing' WHERE id = ?1")
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
async fn invalid_print_metric_is_rejected_by_sqlite_constraint() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let created = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap();
    let Database::Sqlite(pool) = &database else {
        panic!("expected SQLite database");
    };

    let err = sqlx::query("UPDATE jobs SET progress_percent = 101 WHERE id = ?1")
        .bind(created.job.id.to_string())
        .execute(pool)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("CHECK constraint failed"));
}

#[tokio::test]
async fn job_repository_create_rolls_back_command_when_job_insert_fails() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();

    let err = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, ""))
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::Database(_)));
    assert_eq!(commands.count().await.unwrap(), 0);
}
