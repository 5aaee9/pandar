use pandar_core::{AgentId, CommandId, JobId, JobStatus, PrintStatus};
use serde_json::Value;

use super::*;
use crate::repositories::{ApplyPrintReport, CreatePrintJob, PrintReportDiagnostic};

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

#[tokio::test]
async fn print_command_created_by_job_transaction_has_linked_job() {
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
    let command = commands
        .next_queued_for_agent(tenant.id, agent.id)
        .await
        .unwrap()
        .unwrap();
    let payload: Value = serde_json::from_str(&command.payload_json).unwrap();

    assert_eq!(command.kind, "print_project_file");
    assert_eq!(command.id, created.job.command_id);
    assert_eq!(command.printer_id.as_deref(), Some(printer_id.as_str()));
    assert_eq!(payload["job_id"], created.job.id.to_string());
    assert_eq!(payload["artifact_id"], created.artifact.id);
    assert_eq!(payload["printer_id"], printer_id);
    assert!(
        payload["serial_number"]
            .as_str()
            .unwrap()
            .starts_with("serial-")
    );
    assert_eq!(
        jobs.get_for_tenant(tenant.id, created.job.id)
            .await
            .unwrap()
            .unwrap()
            .job
            .command_id,
        command.id
    );
}

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

fn create_input(
    tenant_id: pandar_core::TenantId,
    agent_id: AgentId,
    printer_id: &str,
    artifact_id: &str,
) -> CreatePrintJob {
    create_input_with_filename(tenant_id, agent_id, printer_id, artifact_id, "plate.3mf")
}

fn create_input_with_filename(
    tenant_id: pandar_core::TenantId,
    agent_id: AgentId,
    printer_id: &str,
    artifact_id: &str,
    filename: &str,
) -> CreatePrintJob {
    CreatePrintJob {
        tenant_id,
        printer_id: printer_id.to_string(),
        agent_id,
        artifact_id: artifact_id.to_string(),
        artifact_filename: filename.to_string(),
        artifact_content_type: "model/3mf".to_string(),
        artifact_size_bytes: 42,
        artifact_storage_path: format!("{tenant_id}/{artifact_id}/{filename}"),
        plate_id: 1,
        use_ams: true,
        flow_cali: false,
        timelapse: false,
    }
}

const OBSERVED_AT: &str = "2026-06-22T00:00:00Z";

fn report_input(
    tenant_id: pandar_core::TenantId,
    agent_id: AgentId,
    printer_id: &str,
    job_id: Option<JobId>,
    artifact_id: Option<String>,
    gcode_state: &str,
) -> ApplyPrintReport {
    ApplyPrintReport {
        tenant_id,
        agent_id,
        serial: format!("serial-{printer_id}"),
        job_id,
        artifact_id,
        subtask_id: None,
        gcode_file: None,
        subtask_name: None,
        gcode_state: Some(gcode_state.to_string()),
        percent: Some(42),
        remaining_time_minutes: Some(60),
        current_layer: Some(3),
        total_layers: Some(9),
        diagnostics: Vec::new(),
        observed_at: OBSERVED_AT.to_string(),
    }
}

fn diagnostic(kind: &str, code: &str, message: &str) -> PrintReportDiagnostic {
    PrintReportDiagnostic {
        kind: kind.to_string(),
        severity: if kind == "print_error" {
            "error".to_string()
        } else {
            "warning".to_string()
        },
        code: Some(code.to_string()),
        message: message.to_string(),
        payload_json: format!(r#"{{"code":"{code}","message":"{message}"}}"#),
    }
}

async fn machine_event_count(database: &Database) -> i64 {
    let Database::Sqlite(pool) = database else {
        panic!("expected SQLite database");
    };
    sqlx::query_scalar("SELECT COUNT(*) FROM machine_events")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn printer_level_machine_event_count(database: &Database) -> i64 {
    let Database::Sqlite(pool) = database else {
        panic!("expected SQLite database");
    };
    sqlx::query_scalar("SELECT COUNT(*) FROM machine_events WHERE job_id IS NULL")
        .fetch_one(pool)
        .await
        .unwrap()
}
