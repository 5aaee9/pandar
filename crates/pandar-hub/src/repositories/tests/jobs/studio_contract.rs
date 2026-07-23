use super::*;

fn actor() -> crate::repositories::AuditActor {
    crate::repositories::AuditActor {
        actor_type: "system".to_owned(),
        user_id: None,
        metadata: None,
    }
}

#[tokio::test]
async fn studio_submission_ids_are_positive_tenant_scoped_and_metadata_is_exact() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let tenant = tenants.create("studio-a", "Studio A").await.unwrap();
    let other = tenants.create("studio-b", "Studio B").await.unwrap();
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
    let first = jobs
        .create_studio_print_job_with_audit(
            create_input(tenant.id, agent.id, &printer_id, "studio-first"),
            metadata.clone(),
            actor(),
        )
        .await
        .unwrap();
    let second = jobs
        .create_studio_print_job_with_audit(
            create_input(tenant.id, agent.id, &printer_id, "studio-second"),
            metadata.clone(),
            actor(),
        )
        .await
        .unwrap();
    let other_first = jobs
        .create_studio_print_job_with_audit(
            create_input(other.id, other_agent.id, &other_printer_id, "studio-other"),
            metadata.clone(),
            actor(),
        )
        .await
        .unwrap();

    assert_eq!(first.job.studio_submission_id.get(), 1);
    assert_eq!(second.job.studio_submission_id.get(), 2);
    assert_eq!(other_first.job.studio_submission_id.get(), 1);
    assert_eq!(first.job.plate_index, 1);
    assert_eq!(first.job.studio_metadata.as_ref(), Some(&metadata));
    let studio_command = commands
        .get_for_tenant(tenant.id, first.job.command_id)
        .await
        .unwrap()
        .unwrap();
    let studio_payload: serde_json::Value =
        serde_json::from_str(&studio_command.payload_json).unwrap();
    assert!(studio_payload["studio_metadata"].is_object());

    let web = jobs
        .create_print_job(create_input(tenant.id, agent.id, &printer_id, "web"))
        .await
        .unwrap();
    assert_eq!(web.job.studio_submission_id.get(), 3);
    assert!(web.job.studio_metadata.is_none());
    let web_command = commands
        .get_for_tenant(tenant.id, web.job.command_id)
        .await
        .unwrap()
        .unwrap();
    let web_payload: serde_json::Value = serde_json::from_str(&web_command.payload_json).unwrap();
    assert!(web_payload["studio_metadata"].is_null());
}

#[tokio::test]
async fn exhausted_studio_submission_sequence_fails_without_partial_rows_or_wraparound() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let tenant = tenants
        .create("studio-exhausted", "Studio Exhausted")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let Database::Sqlite(pool) = &database else {
        panic!("expected SQLite database");
    };
    sqlx::query(
        "INSERT INTO studio_submission_sequences (tenant_id, last_id) VALUES (?1, 2147483647)",
    )
    .bind(tenant.id.to_string())
    .execute(pool)
    .await
    .unwrap();

    let error = jobs
        .create_studio_print_job_with_audit(
            create_input(tenant.id, agent.id, &printer_id, "must-rollback"),
            crate::test_support::studio_metadata_for_tests(),
            actor(),
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        crate::repositories::RepositoryError::StudioSubmissionIdExhausted
    ));
    assert_eq!(commands.count().await.unwrap(), 0);
    for table in ["jobs", "job_artifacts", "audit_events"] {
        let query = format!("SELECT COUNT(*) FROM {table}");
        let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(query))
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "{table} should roll back");
    }
    let last_id: i64 =
        sqlx::query_scalar("SELECT last_id FROM studio_submission_sequences WHERE tenant_id = ?1")
            .bind(tenant.id.to_string())
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(last_id, i64::from(i32::MAX));
}

#[tokio::test]
async fn queued_studio_cancel_is_atomic_idempotent_and_never_dispatches() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let audit = crate::repositories::AuditEventRepository::new(database.clone());
    let tenant = tenants
        .create("studio-cancel", "Studio Cancel")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let created = jobs
        .create_studio_print_job_with_audit(
            create_input(tenant.id, agent.id, &printer_id, "cancel-me"),
            crate::test_support::studio_metadata_for_tests(),
            actor(),
        )
        .await
        .unwrap();

    let cancelled = jobs
        .cancel_studio_print_with_audit(tenant.id, created.job.studio_submission_id, actor())
        .await
        .unwrap();
    assert_eq!(cancelled.job.status, JobStatus::Cancelled);
    assert_eq!(cancelled.job.print.status, PrintStatus::Cancelled);
    let command = commands
        .get_for_tenant(tenant.id, created.job.command_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(command.status, pandar_core::CommandStatus::Cancelled);
    assert!(
        commands
            .next_queued_for_agent(tenant.id, agent.id)
            .await
            .unwrap()
            .is_none()
    );

    let again = jobs
        .cancel_studio_print_with_audit(tenant.id, created.job.studio_submission_id, actor())
        .await
        .unwrap();
    assert_eq!(again.job.status, JobStatus::Cancelled);
    assert_eq!(
        audit
            .list_for_tenant(tenant.id)
            .await
            .unwrap()
            .iter()
            .filter(|event| event.action == "job.cancel")
            .count(),
        1
    );
}

#[tokio::test]
async fn studio_cancel_after_dispatch_is_explicitly_too_late() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let tenant = tenants
        .create("studio-cancel-late", "Studio Cancel Late")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let created = jobs
        .create_studio_print_job_with_audit(
            create_input(tenant.id, agent.id, &printer_id, "too-late"),
            crate::test_support::studio_metadata_for_tests(),
            actor(),
        )
        .await
        .unwrap();
    jobs.mark_print_sent(created.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();

    let error = jobs
        .cancel_studio_print_with_audit(tenant.id, created.job.studio_submission_id, actor())
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        crate::repositories::RepositoryError::StudioCancellationTooLate
    ));
    let command = commands
        .get_for_tenant(tenant.id, created.job.command_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(command.status, pandar_core::CommandStatus::Sent);
    let job = jobs
        .get_for_tenant(tenant.id, created.job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.job.status, JobStatus::Sent);
    assert_eq!(job.job.print.status, PrintStatus::Pending);
}

#[tokio::test]
async fn file_sqlite_concurrent_creates_allocate_each_id_once() {
    let state = crate::AppState::file_sqlite_for_tests().await.unwrap();
    let tenant = state
        .tenants()
        .create("studio-concurrent", "Studio Concurrent")
        .await
        .unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture(
        state.database(),
        tenant.id,
        agent.id,
    )
    .await
    .unwrap();
    let mut tasks = Vec::new();
    for index in 0..12 {
        let state = state.clone();
        let printer_id = printer_id.clone();
        tasks.push(tokio::spawn(async move {
            state
                .jobs()
                .create_studio_print_job_with_audit(
                    create_input(
                        tenant.id,
                        agent.id,
                        &printer_id,
                        &format!("concurrent-{index}"),
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

#[tokio::test]
async fn file_sqlite_studio_submission_ids_and_lookup_survive_reconnect() {
    let temp_dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", temp_dir.path().join("pandar.db").display());
    let config = crate::db::DatabaseConfig::from_url(&url).unwrap();
    let database = crate::db::Database::connect(&config).await.unwrap();
    database.migrate().await.unwrap();
    let tenants = crate::repositories::TenantRepository::new(database.clone());
    let agents = crate::repositories::AgentRepository::new(database.clone());
    let jobs = crate::repositories::JobRepository::new(database.clone());
    let tenant = tenants
        .create("studio-reconnect", "Studio Reconnect")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id =
        crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
            .await
            .unwrap();
    let first = jobs
        .create_print_job(create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "before-reconnect-first",
        ))
        .await
        .unwrap();
    jobs.create_print_job(create_input(
        tenant.id,
        agent.id,
        &printer_id,
        "before-reconnect-second",
    ))
    .await
    .unwrap();

    drop(jobs);
    drop(agents);
    drop(tenants);
    let crate::db::Database::Sqlite(pool) = &database else {
        panic!("expected SQLite database");
    };
    pool.close().await;
    drop(database);

    let reconnected = crate::db::Database::connect(&config).await.unwrap();
    reconnected.migrate().await.unwrap();
    let jobs = crate::repositories::JobRepository::new(reconnected);
    let after_restart = jobs
        .create_print_job(create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "after-reconnect",
        ))
        .await
        .unwrap();

    assert_eq!(after_restart.job.studio_submission_id.get(), 3);
    assert_eq!(
        jobs.get_by_studio_submission_id(tenant.id, first.job.studio_submission_id)
            .await
            .unwrap()
            .unwrap()
            .job
            .id,
        first.job.id
    );
}
