use pandar_core::StudioSubmissionId;

use super::*;

#[tokio::test]
async fn sqlite_model_task_lookup_preserves_metadata_and_tenant_scope() {
    exercise_model_task_lookup(sqlite_database().await, "sqlite-model-task").await;
}

#[tokio::test]
async fn postgres_model_task_lookup_preserves_metadata_and_tenant_scope_when_configured() {
    let Some(database) = postgres::postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    exercise_model_task_lookup(database.clone(), "postgres-model-task").await;
}

async fn exercise_model_task_lookup(database: Database, slug: &str) {
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let tenant = tenants
        .create(&format!("{slug}-a"), "Model Task A")
        .await
        .unwrap();
    let other = tenants
        .create(&format!("{slug}-b"), "Model Task B")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let other_agent = agents.create(other.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let other_printer_id = crate::repositories::test_helpers::insert_printer_fixture(
        &database,
        other.id,
        other_agent.id,
    )
    .await
    .unwrap();
    let metadata = crate::test_support::studio_metadata_for_tests();
    let created = jobs
        .create_studio_print_job_with_audit(
            super::jobs::create_input(tenant.id, agent.id, &printer_id, "model-task-a"),
            metadata.clone(),
            actor(),
        )
        .await
        .unwrap();
    let other_created = jobs
        .create_studio_print_job_with_audit(
            super::jobs::create_input(other.id, other_agent.id, &other_printer_id, "model-task-b"),
            metadata.clone(),
            actor(),
        )
        .await
        .unwrap();
    let id = StudioSubmissionId::try_from(1_i64).unwrap();

    let found = jobs
        .get_by_studio_submission_id(tenant.id, id)
        .await
        .unwrap()
        .unwrap();
    let other_found = jobs
        .get_by_studio_submission_id(other.id, id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.job.id, created.job.id);
    assert_eq!(other_found.job.id, other_created.job.id);
    assert_ne!(found.job.id, other_found.job.id);
    assert_eq!(found.job.studio_metadata.as_ref(), Some(&metadata));
    assert_eq!(other_found.job.studio_metadata.as_ref(), Some(&metadata));
    assert!(
        jobs.get_by_studio_submission_id(tenant.id, StudioSubmissionId::try_from(2_i64).unwrap(),)
            .await
            .unwrap()
            .is_none()
    );
}

fn actor() -> crate::repositories::AuditActor {
    crate::repositories::AuditActor::tenant_token(None, "model-task-test", vec!["plugin:studio"])
}
