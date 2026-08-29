use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use super::*;

#[tokio::test]
async fn cleanup_execute_commits_deletion_queue_when_storage_delete_fails() {
    exercise_cleanup_deletion_failure(sqlite_database().await).await;
}

pub(in crate::repositories::tests) async fn exercise_cleanup_deletion_failure(database: Database) {
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let commands = CommandRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let suffix = uuid::Uuid::new_v4();
    let tenant = tenants
        .create(&format!("cleanup-failure-{suffix}"), "Cleanup Failure")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let terminal = jobs
        .create_print_job(crate::repositories::tests::jobs::create_input(
            tenant.id,
            agent.id,
            &printer_id,
            &format!("terminal-artifact-{suffix}"),
        ))
        .await
        .unwrap();
    jobs.mark_print_sent(terminal.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    jobs.mark_print_succeeded(terminal.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    jobs.apply_print_report(crate::repositories::tests::jobs::report_input(
        tenant.id,
        agent.id,
        &printer_id,
        Some(terminal.job.id),
        None,
        "FINISH",
    ))
    .await
    .unwrap();
    make_old(
        &database,
        &terminal.job.id.to_string(),
        &terminal.job.command_id.to_string(),
    )
    .await;
    let storage_path = terminal.artifact.storage_path.clone();
    let storage = RecordingArtifactStorage::failing();

    let err = cleanup_database(
        &database,
        Some(&storage),
        CleanupOptions::default(),
        CleanupMode::Execute,
    )
    .await
    .unwrap_err();

    let message = format!("{err:#}");
    assert!(message.contains("delete failed"));
    assert!(!message.contains("storage/"));
    assert!(!message.contains(&terminal.artifact.id));
    assert_eq!(storage.deleted(), vec![storage_path.clone()]);
    assert_eq!(artifact_count(&database).await, 0);
    assert!(
        jobs.get_for_tenant(tenant.id, terminal.job.id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(commands.count().await.unwrap(), 0);
    assert_eq!(
        crate::artifacts::lifecycle::queued_deletion_count(&database)
            .await
            .unwrap(),
        1
    );
    let deletion = crate::entities::artifact_deletions::Entity::find()
        .filter(crate::entities::artifact_deletions::Column::StoragePath.eq(storage_path.clone()))
        .one(&database.sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(deletion.attempts, 1);

    let retry_storage = RecordingArtifactStorage::default();
    crate::artifacts::lifecycle::drain_deletions(&database, &retry_storage)
        .await
        .unwrap();
    assert_eq!(retry_storage.deleted(), vec![storage_path]);
    assert_eq!(
        crate::artifacts::lifecycle::queued_deletion_count(&database)
            .await
            .unwrap(),
        0
    );
}
