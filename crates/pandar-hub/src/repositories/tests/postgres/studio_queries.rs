use pandar_core::{JobStatus, StudioSubmissionId};

use super::studio_contract::input;
use super::*;
use crate::repositories::{StudioTaskQuery, StudioTaskStatus};

#[tokio::test]
async fn postgres_studio_filters_pagination_ids_and_tenant_isolation_survive_reconnect_when_configured()
 {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let tenant = tenants
        .create("studio-query-a", "Studio Query A")
        .await
        .unwrap();
    let other_tenant = tenants
        .create("studio-query-b", "Studio Query B")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let other_agent = agents.create(other_tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    let second_printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    let other_printer_id = insert_printer_fixture(&database, other_tenant.id, other_agent.id)
        .await
        .unwrap();

    let oldest = jobs
        .create_print_job(input(
            tenant.id,
            agent.id,
            &printer_id,
            "studio-query-oldest",
        ))
        .await
        .unwrap();
    let newest = jobs
        .create_print_job(input(
            tenant.id,
            agent.id,
            &printer_id,
            "studio-query-newest",
        ))
        .await
        .unwrap();
    let Database::Postgres(pool) = &database else {
        panic!("expected PostgreSQL database");
    };
    for (job, created_at) in [
        (&oldest, "2026-07-20T00:00:00.1Z"),
        (&newest, "2026-07-20T00:00:00.11Z"),
    ] {
        sqlx::query("UPDATE jobs SET created_at = $1 WHERE id = $2")
            .bind(created_at)
            .bind(job.job.id.to_string())
            .execute(pool)
            .await
            .unwrap();
    }
    jobs.create_print_job(input(
        tenant.id,
        agent.id,
        &second_printer_id,
        "studio-query-second-printer",
    ))
    .await
    .unwrap();
    let other = jobs
        .create_print_job(input(
            other_tenant.id,
            other_agent.id,
            &other_printer_id,
            "studio-query-other-tenant",
        ))
        .await
        .unwrap();
    jobs.mark_for_command(
        oldest.job.command_id,
        JobStatus::Failed,
        Some("fixture dispatch failure".to_owned()),
    )
    .await
    .unwrap();

    let failed = jobs
        .list_studio_tasks(
            tenant.id,
            StudioTaskQuery {
                printer_id: Some(printer_id.clone()),
                status: Some(StudioTaskStatus::Failed),
                offset: 0,
                limit: 20,
            },
        )
        .await
        .unwrap();
    assert_eq!(failed.total, 1);
    assert_eq!(failed.jobs[0].job.id, oldest.job.id);

    let page = jobs
        .list_studio_tasks(
            tenant.id,
            StudioTaskQuery {
                printer_id: Some(printer_id.clone()),
                status: None,
                offset: 1,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.jobs.len(), 1);
    assert_eq!(page.jobs[0].job.id, oldest.job.id);
    assert_eq!(newest.job.studio_submission_id.get(), 2);

    let tenant_first = jobs
        .get_by_studio_submission_id(tenant.id, StudioSubmissionId::try_from(1_i64).unwrap())
        .await
        .unwrap()
        .unwrap();
    let other_first = jobs
        .get_by_studio_submission_id(
            other_tenant.id,
            StudioSubmissionId::try_from(1_i64).unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(tenant_first.job.id, oldest.job.id);
    assert_eq!(other_first.job.id, other.job.id);

    pool.close().await;
    drop(jobs);
    drop(agents);
    drop(tenants);
    drop(database);

    let url = std::env::var("PANDAR_TEST_POSTGRES_URL").unwrap();
    let config = DatabaseConfig::from_url(url).unwrap();
    let reconnected = Database::connect(&config).await.unwrap();
    reconnected.migrate().await.unwrap();
    let jobs = JobRepository::new(reconnected);
    let after_restart = jobs
        .create_print_job(input(
            tenant.id,
            agent.id,
            &printer_id,
            "studio-query-after-restart",
        ))
        .await
        .unwrap();
    assert_eq!(after_restart.job.studio_submission_id.get(), 4);
    assert_eq!(
        jobs.get_by_studio_submission_id(tenant.id, oldest.job.studio_submission_id)
            .await
            .unwrap()
            .unwrap()
            .job
            .id,
        oldest.job.id
    );
}

#[tokio::test]
async fn postgres_studio_task_count_and_page_share_one_snapshot_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let tenant = tenants
        .create("studio-query-snapshot", "Studio Query Snapshot")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    let first = jobs
        .create_print_job(input(
            tenant.id,
            agent.id,
            &printer_id,
            "studio-query-snapshot-first",
        ))
        .await
        .unwrap();

    let mut pause = crate::repositories::studio_task_test_pause::install();
    let list_jobs = jobs.clone();
    let list_printer_id = printer_id.clone();
    let list = tokio::spawn(async move {
        list_jobs
            .list_studio_tasks(
                tenant.id,
                StudioTaskQuery {
                    printer_id: Some(list_printer_id),
                    status: None,
                    offset: 0,
                    limit: 20,
                },
            )
            .await
    });
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        pause.wait_until_counted(),
    )
    .await
    .expect("Studio task query must reach its post-count pause");
    jobs.create_print_job(input(
        tenant.id,
        agent.id,
        &printer_id,
        "studio-query-snapshot-second",
    ))
    .await
    .unwrap();
    pause.release();

    let page = list.await.unwrap().unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.jobs.len(), 1);
    assert_eq!(page.jobs[0].job.id, first.job.id);
}
