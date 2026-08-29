use pandar_core::{JobStatus, PrintStatus};

use super::*;
use crate::repositories::{ApplyPrintReport, ArtifactQuotaLimits, CreatePrintJob};

mod artifact_lifecycle;

#[tokio::test]
async fn postgres_pending_print_jobs_become_stalled_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    crate::repositories::tests::jobs::stalled::exercise_stalled_print_jobs(
        database.clone(),
        TenantRepository::new(database.clone()),
        AgentRepository::new(database.clone()),
        JobRepository::new(database),
    )
    .await;
}

#[tokio::test]
async fn postgres_print_report_correlates_bambu_submission_id_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    crate::repositories::tests::jobs::submission_correlation::exercise_submission_id_correlation(
        database.clone(),
        TenantRepository::new(database.clone()),
        AgentRepository::new(database.clone()),
        JobRepository::new(database),
    )
    .await;
}

#[tokio::test]
async fn postgres_deletes_one_clearable_job_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let spool = tempfile::tempdir().unwrap();
    let storage = crate::artifacts::FilesystemArtifactStorage::new(
        spool.path(),
        crate::artifacts::DEFAULT_MAX_ARTIFACT_BYTES,
    )
    .unwrap();

    crate::repositories::tests::jobs::delete::exercise_delete_job(
        database.clone(),
        TenantRepository::new(database.clone()),
        AgentRepository::new(database.clone()),
        CommandRepository::new(database.clone()),
        JobRepository::new(database),
        &storage,
    )
    .await;
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
            artifact_metadata_json: None,
            plate_id: 1,
            use_ams: true,
            bed_leveling: false,
            auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
            flow_cali: false,
            auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
            auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
            timelapse: false,
            ams_mapping_json: None,
            ams_mapping2_json: None,
            ams_mapping_info_json: None,
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
            artifact_metadata_json: None,
            plate_id: 1,
            use_ams: false,
            bed_leveling: false,
            auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
            flow_cali: false,
            auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
            auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
            timelapse: false,
            ams_mapping_json: None,
            ams_mapping2_json: None,
            ams_mapping_info_json: None,
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
async fn postgres_print_report_reconciliation_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
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
            artifact_metadata_json: None,
            plate_id: 1,
            use_ams: true,
            bed_leveling: false,
            auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
            flow_cali: false,
            auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
            auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
            timelapse: false,
            ams_mapping_json: None,
            ams_mapping2_json: None,
            ams_mapping_info_json: None,
        })
        .await
        .unwrap();
    let input = ApplyPrintReport {
        tenant_id: tenant.id,
        agent_id: agent.id,
        serial: format!("serial-{printer_id}"),
        task_id: Some(created.job.id.to_string()),
        job_id: Some(created.job.id),
        print_error: None,
        printer_job_id: None,
        job_attr: None,
        artifact_id: None,
        subtask_id: None,
        gcode_file: Some("plate.3mf".to_string()),
        subtask_name: None,
        gcode_state: Some("RUNNING".to_string()),
        percent: Some(50),
        speed_level: Some(2),
        remaining_time_minutes: Some(30),
        current_layer: Some(4),
        total_layers: Some(8),
        hms: None,
        diagnostics: Vec::new(),
        printer_materials_json: String::new(),
        observed_at: "2026-06-22T00:00:00Z".to_string(),
    };

    let first = jobs.apply_print_report(input.clone()).await.unwrap();
    let second = jobs.apply_print_report(input).await.unwrap();

    let job = first.job.unwrap().job;
    assert!(first.changed);
    assert!(first.inserted_job_events);
    assert_eq!(job.status, JobStatus::Queued);
    assert_eq!(job.print.status, PrintStatus::Running);
    assert_eq!(job.print.progress_percent, Some(50));
    assert!(!second.inserted_job_events);
}
