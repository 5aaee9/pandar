use pandar_core::{JobStatus, PrintStatus};

use super::*;
use crate::repositories::{
    ApplyPrintReport, CreatePrintJob,
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
        "TRUNCATE audit_events, api_tokens, user_identities, jobs, job_artifacts, commands, printers, agents, users, tenants",
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
            plate_id: 1,
            use_ams: true,
            flow_cali: false,
            timelapse: false,
        })
        .await
        .unwrap();
    let input = ApplyPrintReport {
        tenant_id: tenant.id,
        agent_id: agent.id,
        serial: format!("serial-{printer_id}"),
        job_id: Some(created.job.id),
        artifact_id: None,
        subtask_id: None,
        gcode_file: Some("plate.3mf".to_string()),
        subtask_name: None,
        gcode_state: Some("RUNNING".to_string()),
        percent: Some(50),
        remaining_time_minutes: Some(30),
        current_layer: Some(4),
        total_layers: Some(8),
        diagnostics: Vec::new(),
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
