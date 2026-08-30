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

#[tokio::test]
async fn deletion_drain_continues_after_individual_storage_failure() {
    exercise_deletion_drain_continues(sqlite_database().await).await;
}

pub(in crate::repositories::tests) async fn exercise_deletion_drain_continues(database: Database) {
    let failed_path = "cleanup/fails.3mf";
    let healthy_path = "cleanup/deletes.3mf";
    let connection = database.sea_orm_connection();
    crate::artifacts::lifecycle::enqueue_deletion(&connection, failed_path)
        .await
        .unwrap();
    crate::artifacts::lifecycle::enqueue_deletion(&connection, healthy_path)
        .await
        .unwrap();
    let storage = RecordingArtifactStorage::failing_path(failed_path);

    let error = crate::artifacts::lifecycle::drain_deletions(&database, &storage)
        .await
        .unwrap_err();

    assert!(format!("{error:#}").contains("delete failed"));
    let mut attempted = storage.deleted();
    attempted.sort();
    let mut expected = vec![failed_path.to_owned(), healthy_path.to_owned()];
    expected.sort();
    assert_eq!(attempted, expected);
    let remaining = crate::entities::artifact_deletions::Entity::find()
        .one(&connection)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(remaining.storage_path, failed_path);
    assert_eq!(remaining.attempts, 1);
    assert!(remaining.last_error.unwrap().contains("delete failed"));
    assert!(remaining.lease_owner.is_none());
    assert!(remaining.lease_expires_at.is_none());
}

#[tokio::test]
async fn concurrent_deletion_drains_claim_each_object_once() {
    exercise_concurrent_deletion_drains(sqlite_database().await).await;
}

pub(in crate::repositories::tests) async fn exercise_concurrent_deletion_drains(
    database: Database,
) {
    let paths = ["cleanup/one.3mf", "cleanup/two.3mf", "cleanup/three.3mf"];
    let connection = database.sea_orm_connection();
    for path in paths {
        crate::artifacts::lifecycle::enqueue_deletion(&connection, path)
            .await
            .unwrap();
    }
    let storage = RecordingArtifactStorage::default();

    let (first, second) = tokio::join!(
        crate::artifacts::lifecycle::drain_deletions(&database, &storage),
        crate::artifacts::lifecycle::drain_deletions(&database, &storage),
    );

    assert_eq!(first.unwrap() + second.unwrap(), paths.len() as u64);
    let mut deleted = storage.deleted();
    deleted.sort();
    let mut expected = paths.map(str::to_owned);
    expected.sort();
    assert_eq!(deleted, expected);
    assert_eq!(
        crate::artifacts::lifecycle::queued_deletion_count(&database)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn deletion_drain_claims_one_bounded_batch() {
    let database = sqlite_database().await;
    let connection = database.sea_orm_connection();
    for index in 0..=64 {
        crate::artifacts::lifecycle::enqueue_deletion(
            &connection,
            &format!("cleanup/bounded-{index}.3mf"),
        )
        .await
        .unwrap();
    }
    let storage = RecordingArtifactStorage::default();

    assert_eq!(
        crate::artifacts::lifecycle::drain_deletions(&database, &storage)
            .await
            .unwrap(),
        64
    );
    assert_eq!(
        crate::artifacts::lifecycle::queued_deletion_count(&database)
            .await
            .unwrap(),
        1
    );
}
