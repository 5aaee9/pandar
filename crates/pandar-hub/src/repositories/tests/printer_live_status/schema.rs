use super::*;

const SQLITE_MIGRATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/migrations/sqlite/20260710000000_web_print_monitor.sql"
));
const POSTGRES_MIGRATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/migrations/postgres/20260710000000_web_print_monitor.sql"
));

#[test]
fn migrations_define_equivalent_defaults_checks_and_nullable_markers() {
    for migration in [SQLITE_MIGRATION, POSTGRES_MIGRATION] {
        for required in [
            "state_revision >= 1",
            "state_revision INTEGER NOT NULL DEFAULT 1",
            "print_task_generation INTEGER NOT NULL DEFAULT 0",
            "print_task_generation >= 0",
            "print_error_generation INTEGER NOT NULL DEFAULT 0",
            "print_error_generation >= 0",
            "print_job_attr INTEGER;",
            "print_error_task_generation INTEGER;",
            "print_error_session_id TEXT;",
            "print_error_received_at TEXT;",
            "current_session_id TEXT;",
        ] {
            assert!(
                migration.replace("BIGINT", "INTEGER").contains(required),
                "missing migration fragment: {required}"
            );
        }
    }
    assert_eq!(
        POSTGRES_MIGRATION.replace("BIGINT", "INTEGER"),
        SQLITE_MIGRATION
    );
}

pub(super) async fn exercise_web_print_monitor_schema(database: Database) {
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database.clone());
    let tenant = tenants
        .create("web-print-monitor", "Web Print Monitor")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    crate::repositories::test_helpers::insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();

    let printer = printers
        .list_with_live_status_for_tenant(tenant.id)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(printer.state_revision, 1);
    assert_eq!(printer.live_status.task_generation, 0);
    assert_eq!(printer.live_status.error_generation, 0);
    assert_eq!(printer.live_status.job_attr, None);
    assert_eq!(printer.live_status.error_task_generation, None);
    assert_eq!(printer.live_status.error_session_id, None);
    assert_eq!(printer.live_status.error_received_at, None);

    exercise_backfill(&database, tenant.id, agent.id).await;
}

async fn exercise_backfill(database: &Database, tenant_id: TenantId, agent_id: AgentId) {
    const COLUMNS: &str = "id, tenant_id, agent_id, serial_number, name, status, created_at, print_task_id, print_subtask_id, print_progress_percent, print_remaining_time_minutes, print_current_layer, print_total_layers, print_gcode_file, print_subtask_name, print_job_id, print_error, print_gcode_state";
    const VALUES: &str = "('evidence-current-layer', {tenant}, {agent}, 'serial-current-layer', 'Evidence', 'offline', '2026-07-10T00:00:00Z', NULL, NULL, NULL, NULL, 1, NULL, NULL, NULL, NULL, NULL, NULL),
('evidence-error', {tenant}, {agent}, 'serial-error', 'Evidence', 'offline', '2026-07-10T00:00:00Z', NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, 7, NULL),
('evidence-gcode-file', {tenant}, {agent}, 'serial-gcode-file', 'Evidence', 'offline', '2026-07-10T00:00:00Z', NULL, NULL, NULL, NULL, NULL, NULL, 'plate.3mf', NULL, NULL, NULL, NULL),
('evidence-gcode-state', {tenant}, {agent}, 'serial-gcode-state', 'Evidence', 'offline', '2026-07-10T00:00:00Z', NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, 'RUNNING'),
('evidence-job-id', {tenant}, {agent}, 'serial-job-id', 'Evidence', 'offline', '2026-07-10T00:00:00Z', NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, 'job', NULL, NULL),
('evidence-progress', {tenant}, {agent}, 'serial-progress', 'Evidence', 'offline', '2026-07-10T00:00:00Z', NULL, NULL, 1, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL),
('evidence-remaining', {tenant}, {agent}, 'serial-remaining', 'Evidence', 'offline', '2026-07-10T00:00:00Z', NULL, NULL, NULL, 1, NULL, NULL, NULL, NULL, NULL, NULL, NULL),
('evidence-subtask-id', {tenant}, {agent}, 'serial-subtask-id', 'Evidence', 'offline', '2026-07-10T00:00:00Z', NULL, 'subtask', NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL),
('evidence-subtask-name', {tenant}, {agent}, 'serial-subtask-name', 'Evidence', 'offline', '2026-07-10T00:00:00Z', NULL, NULL, NULL, NULL, NULL, NULL, NULL, 'Plate', NULL, NULL, NULL),
('evidence-task-id', {tenant}, {agent}, 'serial-task-id', 'Evidence', 'offline', '2026-07-10T00:00:00Z', 'task', NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL),
('evidence-total-layers', {tenant}, {agent}, 'serial-total-layers', 'Evidence', 'offline', '2026-07-10T00:00:00Z', NULL, NULL, NULL, NULL, NULL, 1, NULL, NULL, NULL, NULL, NULL)";
    let (tenant, agent, migration) = match database {
        Database::Sqlite(_) => ("?1", "?2", SQLITE_MIGRATION),
        Database::Postgres(_) => ("$1", "$2", POSTGRES_MIGRATION),
    };
    let sql = format!(
        "INSERT INTO printers ({COLUMNS}) VALUES {}",
        VALUES.replace("{tenant}", tenant).replace("{agent}", agent)
    );
    match database {
        Database::Sqlite(pool) => {
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(tenant_id.to_string())
                .bind(agent_id.to_string())
                .execute(pool)
                .await
                .unwrap();
            sqlx::raw_sql(backfill_statements(migration))
                .execute(pool)
                .await
                .unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(tenant_id.to_string())
                .bind(agent_id.to_string())
                .execute(pool)
                .await
                .unwrap();
            sqlx::raw_sql(backfill_statements(migration))
                .execute(pool)
                .await
                .unwrap();
        }
    }
    assert_backfill(database).await;
}

fn backfill_statements(migration: &'static str) -> &'static str {
    &migration[migration.find("UPDATE printers").unwrap()..]
}

#[derive(sqlx::FromRow)]
struct BackfillRow {
    id: String,
    print_task_generation: i64,
    print_error_generation: i64,
    print_error_task_generation: Option<i64>,
    print_error_session_id: Option<String>,
    print_error_received_at: Option<String>,
}

async fn assert_backfill(database: &Database) {
    let sql = "SELECT id, print_task_generation, print_error_generation, print_error_task_generation, print_error_session_id, print_error_received_at FROM printers WHERE id LIKE 'evidence-%' ORDER BY id";
    let rows: Vec<BackfillRow> = match database {
        Database::Sqlite(pool) => sqlx::query_as(sql).fetch_all(pool).await.unwrap(),
        Database::Postgres(pool) => sqlx::query_as(sql).fetch_all(pool).await.unwrap(),
    };
    assert_eq!(rows.len(), 11);
    assert!(rows.iter().all(|row| row.print_task_generation == 1));
    let error = rows.iter().find(|row| row.id == "evidence-error").unwrap();
    assert_eq!(error.print_error_generation, 1);
    assert_eq!(error.print_error_task_generation, Some(1));
    assert_eq!(error.print_error_session_id, None);
    assert_eq!(error.print_error_received_at, None);
}

#[tokio::test]
async fn legacy_printer_insert_gets_revision_one_and_zero_generations() {
    exercise_web_print_monitor_schema(sqlite_database().await).await;
}
