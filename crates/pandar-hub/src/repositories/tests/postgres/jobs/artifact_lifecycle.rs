use super::*;

#[tokio::test]
async fn postgres_concurrent_artifact_quota_check_allows_only_one_insert_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let slug = format!("quota-{}", uuid::Uuid::new_v4());
    let tenant = tenants.create(&slug, "Quota").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let actor = || crate::repositories::AuditActor {
        actor_type: "system".to_owned(),
        user_id: None,
        metadata: None,
    };
    let quota = ArtifactQuotaLimits {
        tenant_bytes: 42,
        tenant_count: 1,
        global_bytes: 42,
        global_count: 1,
    };
    let first = jobs.create_print_job_with_quota_and_audit(
        crate::repositories::tests::jobs::create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "quota-artifact-1",
        ),
        quota,
        actor(),
    );
    let second = jobs.create_print_job_with_quota_and_audit(
        crate::repositories::tests::jobs::create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "quota-artifact-2",
        ),
        quota,
        actor(),
    );
    let (first, second) = tokio::join!(first, second);
    let outcomes = [first, second];

    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| matches!(result, Err(RepositoryError::ArtifactQuotaExceeded)))
            .count(),
        1
    );
}

#[tokio::test]
async fn postgres_concurrent_committed_artifact_reservations_admit_one_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let tenant = tenants
        .create(
            &format!("reservation-{}", uuid::Uuid::new_v4()),
            "Reservation",
        )
        .await
        .unwrap();
    let quota = ArtifactQuotaLimits {
        tenant_bytes: 42,
        tenant_count: 1,
        global_bytes: 42,
        global_count: 1,
    };
    let first = jobs.reserve_artifact_quota(
        tenant.id,
        "pg-reserved-artifact-1".to_owned(),
        "pg-reservation/artifact-1".to_owned(),
        42,
        quota,
    );
    let second = jobs.reserve_artifact_quota(
        tenant.id,
        "pg-reserved-artifact-2".to_owned(),
        "pg-reservation/artifact-2".to_owned(),
        42,
        quota,
    );
    let (first, second) = tokio::join!(first, second);

    assert_eq!(
        [&first, &second]
            .into_iter()
            .filter(|result| result.is_ok())
            .count(),
        1
    );
    assert_eq!(
        [&first, &second]
            .into_iter()
            .filter(|result| matches!(result, Err(RepositoryError::ArtifactQuotaExceeded)))
            .count(),
        1
    );
    first.or(second).unwrap().release().await.unwrap();
}

#[tokio::test]
async fn postgres_expired_artifact_reservation_releases_before_admission_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    crate::repositories::tests::jobs::reservations::exercise_expired_artifact_reservation(database)
        .await;
}

#[tokio::test]
async fn postgres_committed_artifact_reservation_finalizes_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    crate::repositories::tests::jobs::reservations::exercise_committed_artifact_reservation_finalization(
        database,
    )
    .await;
}

#[tokio::test]
async fn postgres_clear_terminal_and_stalled_jobs_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let spool = tempfile::tempdir().unwrap();
    let storage = crate::artifacts::FilesystemArtifactStorage::new(
        spool.path(),
        crate::artifacts::DEFAULT_MAX_ARTIFACT_BYTES,
    )
    .unwrap();

    crate::repositories::tests::jobs::clear::exercise_clear_jobs(
        database.clone(),
        TenantRepository::new(database.clone()),
        AgentRepository::new(database.clone()),
        CommandRepository::new(database.clone()),
        JobRepository::new(database),
        &storage,
    )
    .await;
}
#[tokio::test]
async fn postgres_artifact_delete_failure_retries_after_job_clear_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    crate::repositories::tests::jobs::clear::lifecycle::exercise_artifact_delete_failure_after_job_clear(
        database,
    )
    .await;
}

#[tokio::test]
async fn postgres_cleanup_deletion_failure_retries_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };

    crate::repositories::tests::cleanup::lifecycle::exercise_cleanup_deletion_failure(database)
        .await;
}
