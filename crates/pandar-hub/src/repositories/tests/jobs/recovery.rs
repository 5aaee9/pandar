use super::*;

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
    let command = commands
        .next_queued_for_agent(tenant.id, agent.id)
        .await
        .unwrap()
        .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&command.payload_json).unwrap();
    assert_eq!(
        payload["artifact_download_path"],
        format!("/api/v1/agents/{}/artifacts/{}", agent.id, safe.artifact.id)
    );
    commands
        .mark_sent(command.id, tenant.id, agent.id)
        .await
        .unwrap();

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
    let source_command = commands
        .next_queued_for_agent(tenant.id, agent.id)
        .await
        .unwrap()
        .unwrap();
    commands
        .mark_sent(source_command.id, tenant.id, agent.id)
        .await
        .unwrap();
    let reprint_command = commands
        .next_queued_for_agent(tenant.id, agent.id)
        .await
        .unwrap()
        .unwrap();
    let reprint_payload: serde_json::Value =
        serde_json::from_str(&reprint_command.payload_json).unwrap();
    assert_eq!(
        reprint_payload["artifact_download_path"],
        format!(
            "/api/v1/agents/{}/artifacts/{}",
            agent.id, source.artifact.id
        )
    );
    commands
        .mark_sent(reprint_command.id, tenant.id, agent.id)
        .await
        .unwrap();

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
    let running_command = commands
        .next_queued_for_agent(tenant.id, agent.id)
        .await
        .unwrap()
        .unwrap();
    commands
        .mark_sent(running_command.id, tenant.id, agent.id)
        .await
        .unwrap();
    let duplicate_command = commands
        .next_queued_for_agent(tenant.id, agent.id)
        .await
        .unwrap()
        .unwrap();
    let duplicate_payload: serde_json::Value =
        serde_json::from_str(&duplicate_command.payload_json).unwrap();
    assert_eq!(
        duplicate_payload["artifact_download_path"],
        format!(
            "/api/v1/agents/{}/artifacts/{}",
            agent.id, running.artifact.id
        )
    );
}
