use super::*;
use crate::cleanup::{CleanupMode, CleanupOptions, cleanup_artifact_rows, cleanup_database};

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

    let dry_run = cleanup_database(&database, CleanupOptions::default(), CleanupMode::DryRun)
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

    let executed = cleanup_database(&database, CleanupOptions::default(), CleanupMode::Execute)
        .await
        .unwrap();
    assert_eq!(executed.jobs, 2);
    assert_eq!(executed.artifacts, 1);
    assert_eq!(executed.artifact_bytes, 42);
    assert_eq!(
        executed.artifact_storage_paths,
        dry_run.artifact_storage_paths
    );
    assert_eq!(artifact_count(&database).await, 4);
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

    cleanup_artifact_rows(&database, &executed.artifact_ids)
        .await
        .unwrap();
    assert_eq!(artifact_count(&database).await, 3);
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

    let dry_run = cleanup_database(&database, CleanupOptions::default(), CleanupMode::DryRun)
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

async fn insert_ticket(
    database: &Database,
    id: &str,
    created_at: &str,
    used_at: Option<&str>,
    revoked_at: Option<&str>,
    expires_at: &str,
) {
    let tenant_id = ensure_cleanup_tenant(database).await;
    match database {
        Database::Sqlite(pool) => {
            sqlx::query(
                "INSERT INTO plugin_login_tickets (id, tenant_id, user_id, ticket_hash, redirect_url, created_at, expires_at, used_at, revoked_at)
                 VALUES (?1, ?2, NULL, ?3, 'http://localhost', ?4, ?5, ?6, ?7)",
            )
            .bind(id)
            .bind(&tenant_id)
            .bind(format!("hash-{id}"))
            .bind(created_at)
            .bind(expires_at)
            .bind(used_at)
            .bind(revoked_at)
            .execute(pool)
            .await
            .unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO plugin_login_tickets (id, tenant_id, user_id, ticket_hash, redirect_url, created_at, expires_at, used_at, revoked_at)
                 VALUES ($1, $2, NULL, $3, 'http://localhost', $4, $5, $6, $7)",
            )
            .bind(id)
            .bind(&tenant_id)
            .bind(format!("hash-{id}"))
            .bind(created_at)
            .bind(expires_at)
            .bind(used_at)
            .bind(revoked_at)
            .execute(pool)
            .await
            .unwrap();
        }
    }
}

async fn insert_tenant_token(
    database: &Database,
    id: &str,
    created_at: &str,
    revoked_at: Option<&str>,
    expires_at: Option<&str>,
) {
    let tenant_id = ensure_cleanup_tenant(database).await;
    match database {
        Database::Sqlite(pool) => {
            sqlx::query(
                "INSERT INTO tenant_tokens (id, tenant_id, name, token_hash, scopes_json, created_by_user_id, created_at, last_used_at, expires_at, revoked_at)
                 VALUES (?1, ?2, ?3, ?4, '[]', NULL, ?5, NULL, ?6, ?7)",
            )
            .bind(id)
            .bind(&tenant_id)
            .bind(id)
            .bind(format!("hash-{id}"))
            .bind(created_at)
            .bind(expires_at)
            .bind(revoked_at)
            .execute(pool)
            .await
            .unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO tenant_tokens (id, tenant_id, name, token_hash, scopes_json, created_by_user_id, created_at, last_used_at, expires_at, revoked_at)
                 VALUES ($1, $2, $3, $4, '[]', NULL, $5, NULL, $6, $7)",
            )
            .bind(id)
            .bind(&tenant_id)
            .bind(id)
            .bind(format!("hash-{id}"))
            .bind(created_at)
            .bind(expires_at)
            .bind(revoked_at)
            .execute(pool)
            .await
            .unwrap();
        }
    }
}

async fn ensure_cleanup_tenant(database: &Database) -> String {
    let tenant_id = "cleanup-token-tenant";
    match database {
        Database::Sqlite(pool) => {
            sqlx::query(
                "INSERT OR IGNORE INTO tenants (id, slug, display_name, created_at) VALUES (?1, 'cleanup-token', 'Cleanup Token', '2025-01-01T00:00:00Z')",
            )
            .bind(tenant_id)
            .execute(pool)
            .await
            .unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO tenants (id, slug, display_name, created_at) VALUES ($1, 'cleanup-token', 'Cleanup Token', '2025-01-01T00:00:00Z') ON CONFLICT (id) DO NOTHING",
            )
            .bind(tenant_id)
            .execute(pool)
            .await
            .unwrap();
        }
    }
    tenant_id.to_owned()
}
