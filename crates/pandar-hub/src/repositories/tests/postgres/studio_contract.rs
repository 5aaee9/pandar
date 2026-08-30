use super::*;
use crate::repositories::{AuditActor, CreatePrintJob};

#[tokio::test]
async fn postgres_studio_ids_metadata_cancel_and_exhaustion_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let tenant = tenants
        .create("studio-contract", "Studio Contract")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    let metadata = crate::test_support::studio_metadata_for_tests();
    let studio = jobs
        .create_studio_print_job_with_audit(
            input(tenant.id, agent.id, &printer_id, "studio"),
            metadata.clone(),
            actor(),
        )
        .await
        .unwrap();
    let web = jobs
        .create_print_job(input(tenant.id, agent.id, &printer_id, "web"))
        .await
        .unwrap();
    assert_eq!(studio.job.studio_submission_id.get(), 1);
    assert_eq!(studio.job.studio_metadata.as_ref(), Some(&metadata));
    assert_eq!(web.job.studio_submission_id.get(), 2);
    assert!(web.job.studio_metadata.is_none());

    let cancelled = jobs
        .cancel_studio_print_with_audit(tenant.id, studio.job.studio_submission_id, actor())
        .await
        .unwrap();
    assert_eq!(cancelled.job.status, pandar_core::JobStatus::Cancelled);
    assert_eq!(
        cancelled.job.print.status,
        pandar_core::PrintStatus::Cancelled
    );
    assert_eq!(
        commands
            .get_for_tenant(tenant.id, studio.job.command_id)
            .await
            .unwrap()
            .unwrap()
            .status,
        pandar_core::CommandStatus::Cancelled
    );

    let Database::Postgres(pool) = &*database else {
        panic!("expected PostgreSQL database");
    };
    sqlx::query("UPDATE studio_submission_sequences SET last_id = 2147483647 WHERE tenant_id = $1")
        .bind(tenant.id.to_string())
        .execute(pool)
        .await
        .unwrap();
    let error = jobs
        .create_studio_print_job_with_audit(
            input(tenant.id, agent.id, &printer_id, "exhausted"),
            metadata,
            actor(),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        RepositoryError::StudioSubmissionIdExhausted
    ));
}

#[tokio::test]
async fn postgres_concurrent_studio_creates_allocate_each_id_once_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let tenant = tenants
        .create("studio-concurrent", "Studio Concurrent")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    let mut tasks = Vec::new();
    for index in 0..12 {
        let database = database.clone();
        let printer_id = printer_id.clone();
        tasks.push(tokio::spawn(async move {
            JobRepository::new(database)
                .create_studio_print_job_with_audit(
                    input(
                        tenant.id,
                        agent.id,
                        &printer_id,
                        &format!("studio-concurrent-{index}"),
                    ),
                    crate::test_support::studio_metadata_for_tests(),
                    actor(),
                )
                .await
                .unwrap()
                .job
                .studio_submission_id
                .get()
        }));
    }
    let mut ids = Vec::new();
    for task in tasks {
        ids.push(task.await.unwrap());
    }
    ids.sort_unstable();
    assert_eq!(ids, (1..=12).collect::<Vec<_>>());
}

pub(super) fn input(
    tenant_id: TenantId,
    agent_id: AgentId,
    printer_id: &str,
    artifact_id: &str,
) -> CreatePrintJob {
    CreatePrintJob {
        tenant_id,
        printer_id: printer_id.to_owned(),
        agent_id,
        artifact_id: artifact_id.to_owned(),
        artifact_filename: "plate.3mf".to_owned(),
        artifact_content_type: "model/3mf".to_owned(),
        artifact_size_bytes: 42,
        artifact_storage_path: format!("{tenant_id}/{artifact_id}/plate.3mf"),
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
    }
}

pub(super) fn actor() -> AuditActor {
    AuditActor::tenant_token(None, "postgres-studio-contract", vec!["*"])
}
