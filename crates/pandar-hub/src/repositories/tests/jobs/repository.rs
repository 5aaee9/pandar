use super::*;
use crate::{Database, repositories::DuplicatePrintJob};

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
async fn job_repository_artifact_for_agent_requires_matching_job_agent() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let assigned_agent = agents.create(tenant.id, "assigned").await.unwrap();
    let other_agent = agents.create(tenant.id, "other").await.unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture(
        &database,
        tenant.id,
        assigned_agent.id,
    )
    .await
    .unwrap();
    let created = jobs
        .create_print_job(create_input(
            tenant.id,
            assigned_agent.id,
            &printer_id,
            "artifact-1",
        ))
        .await
        .unwrap();

    let artifact = jobs
        .artifact_access_for_agent(tenant.id, assigned_agent.id, "artifact-1")
        .await
        .unwrap();

    assert!(matches!(
        artifact,
        AgentArtifactAccess::Allowed(allowed) if allowed == created.artifact
    ));
    assert!(matches!(
        jobs.artifact_access_for_agent(tenant.id, other_agent.id, "artifact-1")
            .await
            .unwrap(),
        AgentArtifactAccess::Forbidden
    ));
    assert!(matches!(
        jobs.artifact_access_for_agent(tenant.id, assigned_agent.id, "missing")
            .await
            .unwrap(),
        AgentArtifactAccess::NotFound
    ));
}

#[tokio::test]
async fn job_repository_metadata_round_trips_through_create_list_and_get() {
    let (database, tenants, agents, _, _, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let mut input = create_input(tenant.id, agent.id, &printer_id, "artifact-1");
    input.artifact_metadata_json = Some(artifact_metadata_json("Widget", 1));

    let created = jobs.create_print_job(input).await.unwrap();
    let listed = jobs.list_for_tenant(tenant.id).await.unwrap();
    let fetched = jobs
        .get_for_tenant(tenant.id, created.job.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        created.artifact.metadata_json,
        Some(artifact_metadata_json("Widget", 1))
    );
    assert_eq!(
        listed[0].artifact.metadata_json,
        created.artifact.metadata_json
    );
    assert_eq!(
        fetched.artifact.metadata_json,
        created.artifact.metadata_json
    );
}

#[tokio::test]
async fn job_repository_missing_metadata_remains_none() {
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

    assert_eq!(created.artifact.metadata_json, None);
}

#[tokio::test]
async fn job_repository_reprint_and_duplicate_reuse_artifact_metadata() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let tenant = tenants.create("acme", "Acme Labs").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let mut input = create_input(tenant.id, agent.id, &printer_id, "artifact-1");
    input.artifact_metadata_json = Some(artifact_metadata_json("Reusable", 2));
    input.ams_mapping_json = Some("[0]".to_owned());
    input.ams_mapping2_json = Some(r#"[{"ams_id":0,"slot_id":0}]"#.to_owned());
    input.ams_mapping_info_json = Some(
        r#"[{"ams":0,"targetColor":"11223344","filamentId":"GFA00","filamentType":"PLA","nozzleId":0}]"#
            .to_owned(),
    );
    let source = jobs.create_print_job(input).await.unwrap();
    commands
        .mark_sent(source.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    commands
        .mark_acknowledged(source.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    commands
        .mark_succeeded(source.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    jobs.mark_for_command(source.job.command_id, JobStatus::Succeeded, None)
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET print_status = 'completed' WHERE id = ?1")
        .bind(source.job.id.to_string())
        .execute(sqlite_pool(&database))
        .await
        .unwrap();

    let reprint = jobs
        .reprint_with_audit(
            tenant.id,
            source.job.id,
            DuplicatePrintJob {
                replace_ams_mappings: true,
                ..DuplicatePrintJob::default()
            },
            None,
            test_audit_actor(),
        )
        .await
        .unwrap();
    let duplicate = jobs
        .duplicate_and_print_with_audit(
            tenant.id,
            source.job.id,
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
            test_audit_actor(),
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
    assert_eq!(reprint.job.ams_mapping_json, None);
    assert_eq!(reprint.job.ams_mapping2_json, None);
    assert_eq!(reprint.job.ams_mapping_info_json, None);
    assert_eq!(duplicate.job.ams_mapping_json, source.job.ams_mapping_json);
}

#[tokio::test]
async fn job_repository_invalid_persisted_metadata_is_data_error() {
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
    sqlx::query("UPDATE job_artifacts SET metadata_json = '{' WHERE id = ?1")
        .bind(&created.artifact.id)
        .execute(sqlite_pool(&database))
        .await
        .unwrap();

    let err = jobs.list_for_tenant(tenant.id).await.unwrap_err();

    assert!(format!("{err:#}").contains("invalid persisted artifact metadata"));
}

fn sqlite_pool(database: &Database) -> &sqlx::SqlitePool {
    let Database::Sqlite(pool) = database else {
        panic!("expected sqlite database");
    };
    pool
}

fn test_audit_actor() -> crate::repositories::AuditActor {
    crate::repositories::AuditActor {
        actor_type: "system".to_owned(),
        user_id: None,
        metadata: None,
    }
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
