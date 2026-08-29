use super::*;

#[tokio::test]
async fn artifact_delete_failure_leaves_durable_retry_after_job_clear_on_sqlite() {
    exercise_artifact_delete_failure_after_job_clear(sqlite_database().await).await;
}

pub(in crate::repositories::tests) async fn exercise_artifact_delete_failure_after_job_clear(
    database: Database,
) {
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let suffix = uuid::Uuid::new_v4();
    let tenant = tenants
        .create(&format!("clear-failure-{suffix}"), "Clear Failure")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let terminal = create_job(
        &jobs,
        tenant.id,
        agent.id,
        &printer_id,
        &format!("terminal-{suffix}"),
    )
    .await;
    succeed(&jobs, terminal.job.command_id, tenant.id, agent.id).await;
    jobs.apply_print_report(report_input(
        tenant.id,
        agent.id,
        &printer_id,
        Some(terminal.job.id),
        None,
        "FINISH",
    ))
    .await
    .unwrap();
    let storage = crate::repositories::tests::cleanup::storage::RecordingArtifactStorage::failing();

    let outcome = jobs
        .clear_for_tenant_with_audit(&storage, tenant.id, AuditActor::no_auth())
        .await
        .unwrap();

    assert_eq!(outcome.deleted_jobs, 1);
    assert!(
        jobs.get_for_tenant(tenant.id, terminal.job.id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(commands.count().await.unwrap(), 0);
    assert_eq!(artifact_count(&database, tenant.id).await, 0);
    assert_eq!(clear_audit_count(&database, tenant.id).await, 1);
    assert_eq!(
        crate::artifacts::lifecycle::queued_deletion_count(&database)
            .await
            .unwrap(),
        1
    );

    let retry_storage =
        crate::repositories::tests::cleanup::storage::RecordingArtifactStorage::default();
    crate::artifacts::lifecycle::drain_deletions(&database, &retry_storage)
        .await
        .unwrap();
    assert_eq!(
        retry_storage.deleted(),
        vec![terminal.artifact.storage_path]
    );
}
