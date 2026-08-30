use std::time::Duration;

use pandar_core::CommandStatus;

use super::studio_contract::{actor, input};
use super::*;

#[tokio::test]
async fn postgres_studio_cancel_and_dispatch_share_one_lock_order_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let tenant = tenants
        .create("studio-lock-order", "Studio Lock Order")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    let created = jobs
        .create_print_job(input(tenant.id, agent.id, &printer_id, "studio-lock-order"))
        .await
        .unwrap();

    let Database::Postgres(pool) = &*database else {
        panic!("expected PostgreSQL database");
    };
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM commands WHERE id = $1 FOR UPDATE")
        .bind(created.job.command_id.to_string())
        .fetch_one(&mut *blocker)
        .await
        .unwrap();

    let dispatch_jobs = jobs.clone();
    let command_id = created.job.command_id;
    let tenant_id = tenant.id;
    let agent_id = agent.id;
    let dispatch = tokio::spawn(async move {
        dispatch_jobs
            .mark_print_sent(command_id, tenant_id, agent_id)
            .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;

    let cancel_jobs = jobs.clone();
    let studio_submission_id = created.job.studio_submission_id;
    let cancel = tokio::spawn(async move {
        cancel_jobs
            .cancel_studio_print_with_audit(tenant_id, studio_submission_id, actor())
            .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    blocker.commit().await.unwrap();

    let (dispatch, cancel) = tokio::time::timeout(Duration::from_secs(5), async {
        (dispatch.await.unwrap(), cancel.await.unwrap())
    })
    .await
    .expect("dispatch and cancellation must serialize without a PostgreSQL deadlock");

    match (dispatch, cancel) {
        (Ok(command), Err(RepositoryError::StudioCancellationTooLate)) => {
            assert_eq!(command.status, CommandStatus::Sent);
        }
        (
            Err(RepositoryError::InvalidCommandTransition {
                from,
                action: "send",
            }),
            Ok(cancelled),
        ) => {
            assert_eq!(from, CommandStatus::Cancelled.as_str());
            assert_eq!(cancelled.job.status, pandar_core::JobStatus::Cancelled);
        }
        (dispatch, cancel) => panic!(
            "expected one domain-level winner without a database error; dispatch={dispatch:?}, cancel={cancel:?}"
        ),
    }
}

#[tokio::test]
async fn postgres_studio_clear_locks_commands_before_jobs_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let jobs = JobRepository::new(database.clone());
    let tenant = tenants
        .create("studio-clear-lock-order", "Studio Clear Lock Order")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    let created = jobs
        .create_print_job(input(
            tenant.id,
            agent.id,
            &printer_id,
            "studio-clear-lock-order",
        ))
        .await
        .unwrap();
    let Database::Postgres(pool) = &*database else {
        panic!("expected PostgreSQL database");
    };
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM commands WHERE id = $1 FOR UPDATE")
        .bind(created.job.command_id.to_string())
        .fetch_one(&mut *blocker)
        .await
        .unwrap();

    let clear_jobs = jobs.clone();
    let clear = tokio::spawn(async move {
        clear_jobs
            .clear_for_tenant_with_audit(
                &crate::repositories::tests::cleanup::storage::RecordingArtifactStorage::default(),
                tenant.id,
                AuditActor::no_auth(),
            )
            .await
    });
    wait_for_blocked_command_lock(pool).await;

    let mut job_probe = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM jobs WHERE id = $1 FOR UPDATE NOWAIT")
        .bind(created.job.id.to_string())
        .fetch_one(&mut *job_probe)
        .await
        .expect("clear must not lock a job while waiting for its command lock");
    job_probe.rollback().await.unwrap();

    let transition_jobs = jobs.clone();
    let transition = tokio::spawn(async move {
        transition_jobs
            .mark_print_sent(created.job.command_id, tenant.id, agent.id)
            .await
    });
    blocker.commit().await.unwrap();
    let (clear, transition) = tokio::time::timeout(Duration::from_secs(5), async {
        (clear.await.unwrap(), transition.await.unwrap())
    })
    .await
    .expect("clear and transition must complete without a PostgreSQL deadlock");
    let clear = clear.unwrap();
    assert_eq!(clear.deleted_jobs, 0);
    assert_eq!(clear.retained_jobs, 1);
    assert_eq!(transition.unwrap().status, CommandStatus::Sent);
}

async fn wait_for_blocked_command_lock(pool: &sqlx::PgPool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE datname = current_database() AND pid <> pg_backend_pid() AND wait_event_type = 'Lock' AND query LIKE '%commands%' AND query LIKE '%FOR UPDATE%')",
            )
            .fetch_one(pool)
            .await
            .unwrap();
            if blocked {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("clear must block on the held command lock");
}
