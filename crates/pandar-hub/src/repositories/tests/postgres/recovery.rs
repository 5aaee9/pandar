use pandar_core::JobStatus;

use super::*;
use crate::repositories::{ApplyPrintReport, CreatePrintJob};

#[tokio::test]
async fn postgres_job_recovery_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let tenant = tenants.create("recovery", "Recovery").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    let source = jobs
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

    let retried = jobs
        .retry_dispatch_with_audit(
            tenant.id,
            source.job.id,
            None,
            crate::repositories::AuditActor {
                actor_type: "system".to_owned(),
                user_id: None,
                metadata: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(retried.job.id, source.job.id);
    assert_ne!(retried.job.command_id, source.job.command_id);

    let completed_source = jobs
        .create_print_job(CreatePrintJob {
            tenant_id: tenant.id,
            printer_id: printer_id.clone(),
            agent_id: agent.id,
            artifact_id: "artifact-2".to_string(),
            artifact_filename: "finished.3mf".to_string(),
            artifact_content_type: "model/3mf".to_string(),
            artifact_size_bytes: 84,
            artifact_storage_path: format!("{}/artifact-2/finished.3mf", tenant.id),
            artifact_metadata_json: None,
            plate_id: 1,
            use_ams: true,
            bed_leveling: false,
            auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
            flow_cali: false,
            auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
            auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
            timelapse: false,
            ams_mapping_json: Some("[0]".to_owned()),
            ams_mapping2_json: Some(r#"[{"ams_id":0,"slot_id":0}]"#.to_owned()),
            ams_mapping_info_json: Some(
                r#"[{"ams":0,"targetColor":"11223344","filamentId":"GFA00","filamentType":"PLA","nozzleId":0}]"#
                    .to_owned(),
            ),
        })
        .await
        .unwrap();
    jobs.apply_print_report(ApplyPrintReport {
        tenant_id: tenant.id,
        agent_id: agent.id,
        serial: format!("serial-{printer_id}"),
        task_id: Some(completed_source.job.id.to_string()),
        job_id: Some(completed_source.job.id),
        print_error: None,
        printer_job_id: None,
        job_attr: None,
        artifact_id: None,
        subtask_id: None,
        gcode_file: Some("finished.3mf".to_string()),
        subtask_name: None,
        gcode_state: Some("FINISH".to_string()),
        percent: Some(100),
        remaining_time_minutes: Some(0),
        current_layer: Some(9),
        total_layers: Some(9),
        hms: None,
        diagnostics: Vec::new(),
        printer_materials_json: String::new(),
        observed_at: "2026-06-22T00:10:00Z".to_string(),
    })
    .await
    .unwrap();

    let reprint = jobs
        .reprint_with_audit(
            tenant.id,
            completed_source.job.id,
            crate::repositories::DuplicatePrintJob {
                printer_id: None,
                plate_id: Some(2),
                use_ams: Some(false),
                bed_leveling: Some(true),
                auto_bed_leveling: Some(pandar_core::PrintCalibrationMode::Auto),
                flow_cali: Some(true),
                auto_flow_cali: Some(pandar_core::PrintCalibrationMode::On),
                auto_offset_cali: Some(pandar_core::PrintCalibrationMode::Auto),
                timelapse: Some(true),
                replace_ams_mappings: true,
                ams_mapping_json: None,
                ams_mapping2_json: None,
                ams_mapping_info_json: None,
            },
            Some("another copy".to_string()),
            crate::repositories::AuditActor {
                actor_type: "system".to_owned(),
                user_id: None,
                metadata: None,
            },
        )
        .await
        .unwrap();
    assert_ne!(reprint.job.id, completed_source.job.id);
    assert_eq!(reprint.job.status, JobStatus::Queued);
    assert_eq!(reprint.artifact.id, completed_source.artifact.id);
    assert_eq!(
        reprint.artifact.storage_path,
        completed_source.artifact.storage_path
    );
    assert_eq!(reprint.job.ams_mapping_json, None);
    assert_eq!(reprint.job.ams_mapping2_json, None);
    assert_eq!(reprint.job.ams_mapping_info_json, None);

    let duplicate = jobs
        .duplicate_and_print_with_audit(
            tenant.id,
            retried.job.id,
            crate::repositories::DuplicatePrintJob {
                printer_id: Some(printer_id),
                plate_id: Some(2),
                use_ams: Some(false),
                bed_leveling: None,
                auto_bed_leveling: None,
                flow_cali: None,
                auto_flow_cali: None,
                auto_offset_cali: None,
                timelapse: None,
                replace_ams_mappings: false,
                ams_mapping_json: None,
                ams_mapping2_json: None,
                ams_mapping_info_json: None,
            },
            crate::repositories::AuditActor {
                actor_type: "system".to_owned(),
                user_id: None,
                metadata: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(duplicate.artifact.id, source.artifact.id);
    assert_eq!(commands.count().await.unwrap(), 5);
}

#[tokio::test]
async fn postgres_job_metadata_round_trips_and_reuses_artifact_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let tenant = tenants
        .create("metadata-postgres", "Metadata Postgres")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    let metadata_json =
        crate::repositories::tests::jobs::artifact_metadata_json("Postgres Metadata", 2);
    let source = jobs
        .create_print_job(CreatePrintJob {
            tenant_id: tenant.id,
            printer_id,
            agent_id: agent.id,
            artifact_id: "artifact-metadata".to_string(),
            artifact_filename: "metadata.3mf".to_string(),
            artifact_content_type: "model/3mf".to_string(),
            artifact_size_bytes: 128,
            artifact_storage_path: format!("{}/artifact-metadata/metadata.3mf", tenant.id),
            artifact_metadata_json: Some(metadata_json.clone()),
            plate_id: 2,
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

    let listed = jobs.list_for_tenant(tenant.id).await.unwrap();
    let fetched = jobs
        .get_for_tenant(tenant.id, source.job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(source.artifact.metadata_json, Some(metadata_json));
    assert_eq!(
        listed[0].artifact.metadata_json,
        source.artifact.metadata_json
    );
    assert_eq!(
        fetched.artifact.metadata_json,
        source.artifact.metadata_json
    );

    jobs.apply_print_report(ApplyPrintReport {
        tenant_id: tenant.id,
        agent_id: agent.id,
        serial: format!("serial-{}", source.job.printer_id),
        task_id: Some(source.job.id.to_string()),
        job_id: Some(source.job.id),
        print_error: None,
        printer_job_id: None,
        job_attr: None,
        artifact_id: None,
        subtask_id: None,
        gcode_file: Some("metadata.3mf".to_string()),
        subtask_name: None,
        gcode_state: Some("FINISH".to_string()),
        percent: Some(100),
        remaining_time_minutes: Some(0),
        current_layer: Some(1),
        total_layers: Some(1),
        hms: None,
        diagnostics: Vec::new(),
        printer_materials_json: String::new(),
        observed_at: "2026-06-24T00:00:00Z".to_string(),
    })
    .await
    .unwrap();

    let reprint = jobs
        .reprint_with_audit(
            tenant.id,
            source.job.id,
            crate::repositories::DuplicatePrintJob::default(),
            None,
            crate::repositories::AuditActor {
                actor_type: "system".to_owned(),
                user_id: None,
                metadata: None,
            },
        )
        .await
        .unwrap();
    let duplicate = jobs
        .duplicate_and_print_with_audit(
            tenant.id,
            source.job.id,
            crate::repositories::DuplicatePrintJob {
                printer_id: None,
                plate_id: None,
                use_ams: None,
                bed_leveling: None,
                auto_bed_leveling: None,
                flow_cali: None,
                auto_flow_cali: None,
                auto_offset_cali: None,
                timelapse: None,
                replace_ams_mappings: false,
                ams_mapping_json: None,
                ams_mapping2_json: None,
                ams_mapping_info_json: None,
            },
            crate::repositories::AuditActor {
                actor_type: "system".to_owned(),
                user_id: None,
                metadata: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        reprint.artifact.metadata_json,
        source.artifact.metadata_json
    );
    assert_eq!(
        duplicate.artifact.metadata_json,
        source.artifact.metadata_json
    );
}
