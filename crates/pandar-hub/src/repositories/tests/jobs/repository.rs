use super::*;

#[tokio::test]
async fn job_repository_list_returns_newest_first() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let first = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap();
    let second = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-2"))
        .await
        .unwrap();

    let listed = jobs.list_for_tenant(tenant.id).await.unwrap();

    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].job.id, second.job.id);
    assert_eq!(listed[1].job.id, first.job.id);
}

#[tokio::test]
async fn job_repository_get_returns_none_for_unknown_job() {
    let (_, tenants, _, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();

    assert_eq!(
        jobs.get_for_tenant(tenant.id, JobId::new()).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn job_repository_rejects_missing_tenant_on_list() {
    let (_, _, _, _, _, jobs) = repositories().await;

    let err = jobs
        .list_for_tenant(pandar_core::TenantId::new())
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::MissingTenant));
}

#[tokio::test]
async fn job_repository_rejects_wrong_tenant_printer() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let acme = tenants.create("acme", "Acme Labs").await.unwrap();
    let beta = tenants.create("beta", "Beta Labs").await.unwrap();
    let agent = agents.create(acme.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, acme.id, agent.id)
            .await
            .unwrap();

    let err = jobs
        .create_print_job(create_input(beta.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::MissingPrinter));
}

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

#[tokio::test]
async fn job_repository_retry_dispatch_requires_safe_pre_physical_failure() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let tenant = tenants.create("retry", "Retry").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let safe = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "safe"))
        .await
        .unwrap();
    jobs.mark_print_sent(safe.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    jobs.mark_print_failed(
        safe.job.command_id,
        tenant.id,
        agent.id,
        "agent offline".to_string(),
    )
    .await
    .unwrap();

    let retried = jobs
        .retry_dispatch_with_audit(
            tenant.id,
            safe.job.id,
            Some("operator retry".to_string()),
            crate::repositories::AuditActor {
                actor_type: "system".to_owned(),
                user_id: None,
                metadata: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(retried.job.id, safe.job.id);
    assert_ne!(retried.job.command_id, safe.job.command_id);
    assert_eq!(retried.job.status, JobStatus::Queued);
    assert_eq!(retried.job.print.status, PrintStatus::Pending);
    assert_eq!(commands.count().await.unwrap(), 2);

    let unsafe_started = jobs
        .create_print_job(create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "unsafe-started",
        ))
        .await
        .unwrap();
    jobs.mark_print_sent(unsafe_started.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    jobs.mark_print_failed(
        unsafe_started.job.command_id,
        tenant.id,
        agent.id,
        "dispatch failed late".to_string(),
    )
    .await
    .unwrap();
    jobs.apply_print_report(report_input(
        tenant.id,
        agent.id,
        &printer_id,
        Some(unsafe_started.job.id),
        None,
        "RUNNING",
    ))
    .await
    .unwrap();

    let err = jobs
        .retry_dispatch_with_audit(
            tenant.id,
            unsafe_started.job.id,
            None,
            crate::repositories::AuditActor {
                actor_type: "system".to_owned(),
                user_id: None,
                metadata: None,
            },
        )
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::RetryNotSafe));
}

#[tokio::test]
async fn job_repository_retry_dispatch_rejects_late_physical_evidence() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let tenant = tenants.create("recovery", "Recovery").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let source = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap();
    jobs.mark_print_sent(source.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    jobs.mark_print_failed(
        source.job.command_id,
        tenant.id,
        agent.id,
        "agent offline".to_owned(),
    )
    .await
    .unwrap();

    let Database::Sqlite(pool) = &database else {
        panic!("expected SQLite database");
    };
    let trigger_sql = format!(
        r#"
        CREATE TEMP TRIGGER retry_late_progress
        AFTER INSERT ON commands
        WHEN NEW.kind = 'print_project_file' AND NEW.id <> '{}'
        BEGIN
            UPDATE jobs SET progress_percent = 1 WHERE id = '{}';
        END
        "#,
        source.job.command_id, source.job.id
    );
    sqlx::query(sqlx::AssertSqlSafe(trigger_sql))
        .execute(pool)
        .await
        .unwrap();

    let err = jobs
        .retry_dispatch_with_audit(
            tenant.id,
            source.job.id,
            Some("retry".to_string()),
            crate::repositories::AuditActor {
                actor_type: "system".to_owned(),
                user_id: None,
                metadata: None,
            },
        )
        .await
        .unwrap_err();

    assert!(matches!(err, RepositoryError::RetryNotSafe));
    assert_eq!(commands.count().await.unwrap(), 1);
}

#[tokio::test]
async fn job_repository_reprint_and_duplicate_create_independent_queued_jobs() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let tenant = tenants.create("recovery", "Recovery").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let source = jobs
        .create_print_job(create_input_with_filename(
            tenant.id,
            agent.id,
            &printer_id,
            "source-artifact",
            "source.3mf",
        ))
        .await
        .unwrap();
    jobs.apply_print_report(report_input(
        tenant.id,
        agent.id,
        &printer_id,
        Some(source.job.id),
        None,
        "FINISH",
    ))
    .await
    .unwrap();

    let reprint = jobs
        .reprint_with_audit(
            tenant.id,
            source.job.id,
            Some("again".to_string()),
            crate::repositories::AuditActor {
                actor_type: "system".to_owned(),
                user_id: None,
                metadata: None,
            },
        )
        .await
        .unwrap();

    assert_ne!(reprint.job.id, source.job.id);
    assert_eq!(reprint.job.status, JobStatus::Queued);
    assert_eq!(reprint.artifact.id, source.artifact.id);
    assert_eq!(reprint.artifact.storage_path, source.artifact.storage_path);
    assert_eq!(commands.count().await.unwrap(), 2);

    let running = jobs
        .create_print_job(create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "running-artifact",
        ))
        .await
        .unwrap();
    jobs.apply_print_report(report_input(
        tenant.id,
        agent.id,
        &printer_id,
        Some(running.job.id),
        None,
        "RUNNING",
    ))
    .await
    .unwrap();

    let duplicate = jobs
        .duplicate_and_print_with_audit(
            tenant.id,
            running.job.id,
            crate::repositories::DuplicatePrintJob {
                printer_id: Some(printer_id.clone()),
                plate_id: Some(2),
                use_ams: Some(false),
                flow_cali: Some(true),
                timelapse: Some(true),
                ams_mapping_json: None,
                ams_mapping2_json: None,
            },
            crate::repositories::AuditActor {
                actor_type: "system".to_owned(),
                user_id: None,
                metadata: None,
            },
        )
        .await
        .unwrap();
    let source_after = jobs
        .get_for_tenant(tenant.id, running.job.id)
        .await
        .unwrap()
        .unwrap();

    assert_ne!(duplicate.job.id, running.job.id);
    assert_eq!(duplicate.artifact.id, running.artifact.id);
    assert_eq!(duplicate.job.print.status, PrintStatus::Pending);
    assert_eq!(duplicate.job.status, JobStatus::Queued);
    assert_eq!(duplicate.job.ams_mapping_json, running.job.ams_mapping_json);
    assert_eq!(source_after.job.print.status, PrintStatus::Running);
    assert_eq!(source_after.job.command_id, running.job.command_id);
}
