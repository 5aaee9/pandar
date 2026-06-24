use super::*;
use crate::cleanup::{CleanupMode, CleanupOptions, cleanup_database};

mod retention;
mod storage;

use retention::{insert_tenant_token, insert_ticket};
use storage::RecordingArtifactStorage;

#[tokio::test]
async fn cleanup_dry_run_does_not_mutate_and_execute_preserves_active_jobs() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    exercise_cleanup(database, tenants, agents, commands, jobs).await;
}

pub(super) async fn exercise_cleanup(
    database: Database,
    tenants: TenantRepository,
    agents: AgentRepository,
    commands: CommandRepository,
    jobs: JobRepository,
) {
    let tenant = tenants.create("cleanup", "Cleanup").await.unwrap();
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
            "terminal-artifact",
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
    let active = jobs
        .create_print_job(crate::repositories::tests::jobs::create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "active-artifact",
        ))
        .await
        .unwrap();
    let shared_terminal = jobs
        .create_print_job(crate::repositories::tests::jobs::create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "shared-terminal-artifact",
        ))
        .await
        .unwrap();
    jobs.mark_print_sent(shared_terminal.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    jobs.mark_print_succeeded(shared_terminal.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    jobs.apply_print_report(crate::repositories::tests::jobs::report_input(
        tenant.id,
        agent.id,
        &printer_id,
        Some(shared_terminal.job.id),
        None,
        "FINISH",
    ))
    .await
    .unwrap();
    let shared_active = jobs
        .create_print_job(crate::repositories::tests::jobs::create_input(
            tenant.id,
            agent.id,
            &printer_id,
            "shared-active-artifact",
        ))
        .await
        .unwrap();

    point_job_at_artifact(
        &database,
        &shared_terminal.job.id.to_string(),
        &terminal.artifact.id,
    )
    .await;
    point_job_at_artifact(
        &database,
        &shared_active.job.id.to_string(),
        &active.artifact.id,
    )
    .await;

    make_old(
        &database,
        &terminal.job.id.to_string(),
        &terminal.job.command_id.to_string(),
    )
    .await;
    make_old(
        &database,
        &shared_terminal.job.id.to_string(),
        &shared_terminal.job.command_id.to_string(),
    )
    .await;

    let storage = RecordingArtifactStorage::default();
    let dry_run = cleanup_database(
        &database,
        Some(&storage),
        CleanupOptions::default(),
        CleanupMode::DryRun,
    )
    .await
    .unwrap();
    assert_eq!(dry_run.jobs, 2);
    assert_eq!(dry_run.artifacts, 1);
    assert_eq!(dry_run.artifact_bytes, 42);
    assert_eq!(dry_run.artifact_storage_paths.len(), 1);
    assert!(
        jobs.get_for_tenant(tenant.id, terminal.job.id)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(commands.count().await.unwrap(), 4);

    assert!(storage.deleted().is_empty());

    let executed = cleanup_database(
        &database,
        Some(&storage),
        CleanupOptions::default(),
        CleanupMode::Execute,
    )
    .await
    .unwrap();
    assert_eq!(executed.jobs, 2);
    assert_eq!(executed.artifacts, 1);
    assert_eq!(executed.artifact_bytes, 42);
    assert_eq!(
        executed.artifact_storage_paths,
        dry_run.artifact_storage_paths
    );
    assert_eq!(storage.deleted(), dry_run.artifact_storage_paths);
    assert_eq!(artifact_count(&database).await, 3);
    assert!(
        jobs.get_for_tenant(tenant.id, terminal.job.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        jobs.get_for_tenant(tenant.id, shared_terminal.job.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        jobs.get_for_tenant(tenant.id, active.job.id)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        jobs.get_for_tenant(tenant.id, shared_active.job.id)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(commands.count().await.unwrap(), 2);
}

#[tokio::test]
async fn cleanup_execute_keeps_artifact_rows_when_storage_delete_fails() {
    let (database, tenants, agents, _, commands, jobs) = repositories().await;
    let tenant = tenants
        .create("cleanup-failure", "Cleanup Failure")
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
            "terminal-artifact",
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
    assert!(!message.contains("terminal-artifact"));
    assert_eq!(storage.deleted(), vec![terminal.artifact.storage_path]);
    assert_eq!(artifact_count(&database).await, 1);
    assert!(
        jobs.get_for_tenant(tenant.id, terminal.job.id)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(commands.count().await.unwrap(), 1);
}

#[tokio::test]
async fn cleanup_uses_terminal_time_for_tokens_and_tickets() {
    let database = sqlite_database().await;
    insert_ticket(
        &database,
        "old-used",
        "2025-01-01T00:00:00Z",
        Some("2025-01-02T00:00:00Z"),
        None,
        "2026-01-01T00:00:00Z",
    )
    .await;
    insert_ticket(
        &database,
        "new-used",
        "2025-01-01T00:00:00Z",
        Some("2999-01-02T00:00:00Z"),
        None,
        "3000-01-01T00:00:00Z",
    )
    .await;
    insert_tenant_token(
        &database,
        "old-revoked",
        "2025-01-01T00:00:00Z",
        Some("2025-01-02T00:00:00Z"),
        None,
    )
    .await;
    insert_tenant_token(
        &database,
        "new-revoked",
        "2025-01-01T00:00:00Z",
        Some("2999-01-02T00:00:00Z"),
        None,
    )
    .await;

    let dry_run = cleanup_database(
        &database,
        None,
        CleanupOptions::default(),
        CleanupMode::DryRun,
    )
    .await
    .unwrap();

    assert_eq!(dry_run.plugin_login_tickets, 1);
    assert_eq!(dry_run.tenant_tokens, 1);
}

async fn make_old(database: &Database, job_id: &str, command_id: &str) {
    let old = "2025-01-01T00:00:00Z";
    match database {
        Database::Sqlite(pool) => {
            sqlx::query("UPDATE jobs SET updated_at = ?1 WHERE id = ?2")
                .bind(old)
                .bind(job_id)
                .execute(pool)
                .await
                .unwrap();
            sqlx::query("UPDATE commands SET updated_at = ?1 WHERE id = ?2")
                .bind(old)
                .bind(command_id)
                .execute(pool)
                .await
                .unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::query("UPDATE jobs SET updated_at = $1 WHERE id = $2")
                .bind(old)
                .bind(job_id)
                .execute(pool)
                .await
                .unwrap();
            sqlx::query("UPDATE commands SET updated_at = $1 WHERE id = $2")
                .bind(old)
                .bind(command_id)
                .execute(pool)
                .await
                .unwrap();
        }
    }
}

async fn point_job_at_artifact(database: &Database, job_id: &str, artifact_id: &str) {
    match database {
        Database::Sqlite(pool) => {
            sqlx::query("UPDATE jobs SET artifact_id = ?1 WHERE id = ?2")
                .bind(artifact_id)
                .bind(job_id)
                .execute(pool)
                .await
                .unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::query("UPDATE jobs SET artifact_id = $1 WHERE id = $2")
                .bind(artifact_id)
                .bind(job_id)
                .execute(pool)
                .await
                .unwrap();
        }
    }
}

async fn artifact_count(database: &Database) -> i64 {
    match database {
        Database::Sqlite(pool) => sqlx::query_scalar("SELECT COUNT(*) FROM job_artifacts")
            .fetch_one(pool)
            .await
            .unwrap(),
        Database::Postgres(pool) => sqlx::query_scalar("SELECT COUNT(*) FROM job_artifacts")
            .fetch_one(pool)
            .await
            .unwrap(),
    }
}
