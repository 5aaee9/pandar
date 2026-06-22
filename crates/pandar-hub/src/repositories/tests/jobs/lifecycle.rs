use super::*;

#[tokio::test]
async fn job_repository_create_print_job_links_artifact_command_and_job() {
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

    assert_eq!(created.artifact.id, "artifact-1");
    assert_eq!(created.job.printer_id, printer_id);
    assert_eq!(created.job.status, JobStatus::Queued);
    assert_eq!(created.job.print.status, PrintStatus::Pending);
    assert_eq!(commands.count().await.unwrap(), 1);
}

#[tokio::test]
async fn print_report_exact_job_id_updates_print_state_without_dispatch_status() {
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

    let applied = jobs
        .apply_print_report(report_input(
            tenant.id,
            agent.id,
            &printer_id,
            Some(created.job.id),
            None,
            "RUNNING",
        ))
        .await
        .unwrap();

    let job = applied.job.unwrap().job;
    assert!(applied.changed);
    assert!(applied.inserted_job_events);
    assert!(!applied.inserted_printer_events);
    assert_eq!(job.status, JobStatus::Queued);
    assert_eq!(job.print.status, PrintStatus::Running);
    assert_eq!(job.print.progress_percent, Some(42));
    assert_eq!(job.print.last_progress_percent, Some(42));
    assert_eq!(job.print.current_layer, Some(3));
    assert_eq!(job.print.last_layer, Some(3));
    assert_eq!(job.print.started_at.as_deref(), Some(OBSERVED_AT));
}

#[tokio::test]
async fn print_report_correlates_by_artifact_and_active_file_fallback() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let artifact = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap();
    let fallback = jobs
        .create_print_job(create_input_with_filename(
            tenant.id,
            agent.id,
            &printer_id,
            "artifact-2",
            "fallback.3mf",
        ))
        .await
        .unwrap();

    let by_artifact = jobs
        .apply_print_report(report_input(
            tenant.id,
            agent.id,
            &printer_id,
            None,
            Some("artifact-1".to_string()),
            "RUNNING",
        ))
        .await
        .unwrap();
    assert_eq!(by_artifact.job.unwrap().job.id, artifact.job.id);

    let by_file = jobs
        .apply_print_report(ApplyPrintReport {
            job_id: None,
            artifact_id: None,
            subtask_id: None,
            gcode_file: Some("/cache/fallback.3mf".to_string()),
            subtask_name: None,
            ..report_input(tenant.id, agent.id, &printer_id, None, None, "RUNNING")
        })
        .await
        .unwrap();
    assert_eq!(by_file.job.unwrap().job.id, fallback.job.id);
}

#[tokio::test]
async fn print_report_file_fallback_ignores_zero_and_ambiguous_matches() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();

    let no_match = jobs
        .apply_print_report(ApplyPrintReport {
            job_id: None,
            artifact_id: None,
            subtask_id: None,
            gcode_file: Some("missing.3mf".to_string()),
            diagnostics: vec![diagnostic("hms", "HMS-1", "fan warning")],
            ..report_input(tenant.id, agent.id, &printer_id, None, None, "RUNNING")
        })
        .await
        .unwrap();
    assert!(no_match.job.is_none());
    assert!(no_match.inserted_printer_events);

    jobs.create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap();
    jobs.create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-2"))
        .await
        .unwrap();
    let ambiguous = jobs
        .apply_print_report(ApplyPrintReport {
            job_id: None,
            artifact_id: None,
            subtask_id: None,
            gcode_file: Some("plate.3mf".to_string()),
            diagnostics: vec![diagnostic("hms", "HMS-2", "chamber warning")],
            ..report_input(tenant.id, agent.id, &printer_id, None, None, "RUNNING")
        })
        .await
        .unwrap();

    assert!(ambiguous.job.is_none());
    assert!(ambiguous.inserted_printer_events);
    assert_eq!(machine_event_count(&database).await, 2);
}

#[tokio::test]
async fn print_report_terminal_transitions_and_replay_are_idempotent() {
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
    jobs.apply_print_report(report_input(
        tenant.id,
        agent.id,
        &printer_id,
        Some(created.job.id),
        None,
        "RUNNING",
    ))
    .await
    .unwrap();

    let finished = jobs
        .apply_print_report(report_input(
            tenant.id,
            agent.id,
            &printer_id,
            Some(created.job.id),
            None,
            "FINISH",
        ))
        .await
        .unwrap();
    let replay = jobs
        .apply_print_report(report_input(
            tenant.id,
            agent.id,
            &printer_id,
            Some(created.job.id),
            None,
            "FINISH",
        ))
        .await
        .unwrap();

    assert_eq!(
        finished.job.unwrap().job.print.status,
        PrintStatus::Completed
    );
    assert!(!replay.inserted_job_events);
    assert_eq!(replay.job.unwrap().job.print.status, PrintStatus::Completed);
    assert_eq!(machine_event_count(&database).await, 3);
}

#[tokio::test]
async fn stale_running_report_does_not_regress_terminal_print_status() {
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
    jobs.apply_print_report(report_input(
        tenant.id,
        agent.id,
        &printer_id,
        Some(created.job.id),
        None,
        "FINISH",
    ))
    .await
    .unwrap();

    let stale = jobs
        .apply_print_report(report_input(
            tenant.id,
            agent.id,
            &printer_id,
            Some(created.job.id),
            None,
            "RUNNING",
        ))
        .await
        .unwrap();

    assert!(!stale.changed);
    assert!(!stale.inserted_job_events);
    assert_eq!(stale.job.unwrap().job.print.status, PrintStatus::Completed);
}

#[tokio::test]
async fn print_report_cancel_and_failed_store_terminal_error() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let cancelled = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-1"))
        .await
        .unwrap();
    let failed = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "artifact-2"))
        .await
        .unwrap();

    jobs.apply_print_report(report_input(
        tenant.id,
        agent.id,
        &printer_id,
        Some(cancelled.job.id),
        None,
        "RUNNING",
    ))
    .await
    .unwrap();
    let cancelled = jobs
        .apply_print_report(report_input(
            tenant.id,
            agent.id,
            &printer_id,
            Some(cancelled.job.id),
            None,
            "IDLE",
        ))
        .await
        .unwrap()
        .job
        .unwrap()
        .job;
    let failed = jobs
        .apply_print_report(ApplyPrintReport {
            diagnostics: vec![diagnostic("print_error", "E-1", "nozzle failure")],
            ..report_input(
                tenant.id,
                agent.id,
                &printer_id,
                Some(failed.job.id),
                None,
                "FAILED",
            )
        })
        .await
        .unwrap()
        .job
        .unwrap()
        .job;

    assert_eq!(cancelled.print.status, PrintStatus::Cancelled);
    assert_eq!(cancelled.print.error.as_deref(), Some("print cancelled"));
    assert_eq!(failed.print.status, PrintStatus::Failed);
    assert_eq!(failed.print.error.as_deref(), Some("nozzle failure"));
}

#[tokio::test]
async fn failed_print_report_uses_non_print_error_diagnostic_message() {
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

    let failed = jobs
        .apply_print_report(ApplyPrintReport {
            diagnostics: vec![diagnostic("hms", "HMS-FAILED", "toolhead fault")],
            ..report_input(
                tenant.id,
                agent.id,
                &printer_id,
                Some(created.job.id),
                None,
                "FAILED",
            )
        })
        .await
        .unwrap()
        .job
        .unwrap()
        .job;

    assert_eq!(failed.print.status, PrintStatus::Failed);
    assert_eq!(failed.print.error.as_deref(), Some("toolhead fault"));
}

#[tokio::test]
async fn uncorrelated_diagnostic_replay_dedupes_printer_event() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let input = ApplyPrintReport {
        job_id: None,
        artifact_id: None,
        subtask_id: None,
        gcode_file: None,
        diagnostics: vec![diagnostic("hms", "HMS-1", "fan warning")],
        ..report_input(tenant.id, agent.id, &printer_id, None, None, "RUNNING")
    };

    let first = jobs.apply_print_report(input.clone()).await.unwrap();
    let second = jobs.apply_print_report(input).await.unwrap();

    assert!(first.job.is_none());
    assert!(first.inserted_printer_events);
    assert!(!second.inserted_printer_events);
    assert_eq!(machine_event_count(&database).await, 1);
    assert_eq!(printer_level_machine_event_count(&database).await, 1);
}
