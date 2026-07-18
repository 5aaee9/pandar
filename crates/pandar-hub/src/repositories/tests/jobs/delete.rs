use super::*;
use crate::{
    artifacts::{DEFAULT_MAX_ARTIFACT_BYTES, FilesystemArtifactStorage},
    repositories::{
        AgentRepository, AuditActor, AuditEventRepository, CommandRepository, DuplicatePrintJob,
        JobRepository, TenantRepository,
    },
};

#[tokio::test]
async fn delete_one_clearable_job_safely_on_sqlite() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let spool = tempfile::tempdir().unwrap();
    let storage = FilesystemArtifactStorage::new(spool.path(), DEFAULT_MAX_ARTIFACT_BYTES).unwrap();

    exercise_delete_job(database, tenants, agents, commands, jobs, &storage).await;
}

pub(in crate::repositories::tests) async fn exercise_delete_job(
    database: Database,
    tenants: TenantRepository,
    agents: AgentRepository,
    commands: CommandRepository,
    jobs: JobRepository,
    storage: &FilesystemArtifactStorage,
) {
    let tenant = tenants.create("delete-job", "Delete Job").await.unwrap();
    let other_tenant = tenants
        .create("delete-job-other", "Delete Job Other")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();

    let target = jobs
        .create_print_job(create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "delete-target",
        ))
        .await
        .unwrap();
    complete(&jobs, tenant.id, agent.id, &printer_id, &target).await;
    let survivor = jobs
        .create_print_job(create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "delete-survivor",
        ))
        .await
        .unwrap();
    complete(&jobs, tenant.id, agent.id, &printer_id, &survivor).await;

    let outcome = jobs
        .delete_clearable_for_tenant_with_audit(
            storage,
            tenant.id,
            target.job.id,
            AuditActor::no_auth(),
        )
        .await
        .unwrap();

    assert_eq!(outcome.deleted_jobs, 1);
    assert_eq!(outcome.retained_jobs, 1);
    assert_eq!(outcome.deleted_commands, 1);
    assert_eq!(outcome.deleted_artifacts, 1);
    assert_eq!(outcome.deleted_artifact_bytes, 42);
    assert!(
        jobs.get_for_tenant(tenant.id, target.job.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        jobs.get_for_tenant(tenant.id, survivor.job.id)
            .await
            .unwrap()
            .is_some()
    );

    let active = jobs
        .create_print_job(create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "delete-active",
        ))
        .await
        .unwrap();
    let error = jobs
        .delete_clearable_for_tenant_with_audit(
            storage,
            tenant.id,
            active.job.id,
            AuditActor::no_auth(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, RepositoryError::JobNotClearable));

    let shared_source = jobs
        .create_print_job(create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "delete-shared",
        ))
        .await
        .unwrap();
    let shared_target = jobs
        .duplicate_and_print_with_audit(
            tenant.id,
            shared_source.job.id,
            DuplicatePrintJob {
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
            AuditActor::no_auth(),
        )
        .await
        .unwrap();
    complete(&jobs, tenant.id, agent.id, &printer_id, &shared_target).await;
    let outcome = jobs
        .delete_clearable_for_tenant_with_audit(
            storage,
            tenant.id,
            shared_target.job.id,
            AuditActor::no_auth(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.deleted_jobs, 1);
    assert_eq!(outcome.deleted_artifacts, 0);
    assert!(
        jobs.get_for_tenant(tenant.id, shared_source.job.id)
            .await
            .unwrap()
            .is_some()
    );

    let wrong_tenant = jobs
        .delete_clearable_for_tenant_with_audit(
            storage,
            other_tenant.id,
            survivor.job.id,
            AuditActor::no_auth(),
        )
        .await
        .unwrap_err();
    assert!(matches!(wrong_tenant, RepositoryError::MissingJob));
    let missing = jobs
        .delete_clearable_for_tenant_with_audit(
            storage,
            tenant.id,
            JobId::new(),
            AuditActor::no_auth(),
        )
        .await
        .unwrap_err();
    assert!(matches!(missing, RepositoryError::MissingJob));

    let events = AuditEventRepository::new(database)
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    let deletes = events
        .iter()
        .filter(|event| event.action == "job.delete")
        .collect::<Vec<_>>();
    assert_eq!(deletes.len(), 2);
    let event = deletes
        .iter()
        .find(|event| event.target_id.as_deref() == Some(&target.job.id.to_string()))
        .unwrap();
    assert_eq!(event.target_type, "job");
    assert!(
        event
            .metadata_json
            .contains("\"artifact_id\":\"delete-target\"")
    );
    assert!(
        event
            .metadata_json
            .contains("\"artifact_filename\":\"plate.3mf\"")
    );
    assert!(
        event
            .metadata_json
            .contains("\"previous_dispatch_status\":\"succeeded\"")
    );
    assert!(
        event
            .metadata_json
            .contains("\"previous_print_status\":\"completed\"")
    );
    assert_eq!(commands.count().await.unwrap(), 3);
}

async fn complete(
    jobs: &JobRepository,
    tenant_id: pandar_core::TenantId,
    agent_id: AgentId,
    printer_id: &str,
    job: &crate::repositories::JobWithArtifact,
) {
    jobs.mark_print_sent(job.job.command_id, tenant_id, agent_id)
        .await
        .unwrap();
    jobs.mark_print_succeeded(job.job.command_id, tenant_id, agent_id)
        .await
        .unwrap();
    jobs.apply_print_report(report_input(
        tenant_id,
        agent_id,
        printer_id,
        Some(job.job.id),
        None,
        "FINISH",
    ))
    .await
    .unwrap();
}
