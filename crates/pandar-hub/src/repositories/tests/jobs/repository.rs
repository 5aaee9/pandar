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
    input.artifact_metadata_json = Some(
        json!({
            "source": "bambu_3mf",
            "display_name": "Widget",
            "default_plate_id": 1,
            "plate_count": 1,
            "plates": [],
            "warnings": []
        })
        .to_string(),
    );

    let created = jobs.create_print_job(input).await.unwrap();
    let listed = jobs.list_for_tenant(tenant.id).await.unwrap();
    let fetched = jobs
        .get_for_tenant(tenant.id, created.job.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        created.artifact.metadata_json,
        Some(
            json!({
                "source": "bambu_3mf",
                "display_name": "Widget",
                "default_plate_id": 1,
                "plate_count": 1,
                "plates": [],
                "warnings": []
            })
            .to_string()
        )
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
    input.artifact_metadata_json = Some(
        json!({
            "source": "bambu_3mf",
            "display_name": "Reusable",
            "default_plate_id": 2,
            "plate_count": 1,
            "plates": [],
            "warnings": []
        })
        .to_string(),
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
        .reprint_with_audit(tenant.id, source.job.id, None, test_audit_actor())
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
                flow_cali: None,
                timelapse: None,
                ams_mapping_json: None,
                ams_mapping2_json: None,
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
