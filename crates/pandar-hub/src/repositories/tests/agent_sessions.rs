use sea_orm::EntityTrait;
use serde::Serialize;

use super::*;
use crate::{
    entities::agents as agent_entities,
    repositories::{PrinterSnapshotUpsert, test_helpers::insert_printer_fixture},
};

#[tokio::test]
async fn sqlite_agent_session_guards_all_agent_owned_mutations() {
    let temp_dir = tempfile::tempdir().unwrap();
    let database_url = format!(
        "sqlite://{}",
        temp_dir
            .path()
            .join("agent-session-guards.sqlite")
            .display()
    );
    let config = DatabaseConfig::from_url(database_url).unwrap();
    let database = Database::connect(&config).await.unwrap();
    database.migrate().await.unwrap();
    let sibling = Database::connect(&config).await.unwrap();

    exercise_exact_session_guards(database, sibling).await;
}

#[tokio::test]
async fn postgres_agent_session_guards_and_lock_order_when_configured() {
    let url = match std::env::var("PANDAR_TEST_POSTGRES_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
            return;
        }
    };
    let Some(database) = super::postgres::postgres_database().await else {
        unreachable!("PostgreSQL URL was read above")
    };
    let sibling = Database::connect(&DatabaseConfig::from_url(url).unwrap())
        .await
        .unwrap();

    exercise_exact_session_guards(database.clone(), sibling).await;
    for mutation in [
        CurrentAgentMutation::Snapshot,
        CurrentAgentMutation::PrintReport,
        CurrentAgentMutation::MaterialSnapshot,
    ] {
        assert_postgres_agent_then_printer_lock_order(&database, mutation).await;
    }
}

#[tokio::test]
async fn sqlite_current_agent_transaction_reserves_writer_before_printer_query() {
    let temp_dir = tempfile::tempdir().unwrap();
    let database_url = format!(
        "sqlite://{}",
        temp_dir.path().join("agent-lock.sqlite").display()
    );
    let config = DatabaseConfig::from_url(database_url).unwrap();
    let database = Database::connect(&config).await.unwrap();
    database.migrate().await.unwrap();
    let (tenant_id, agent_id, session_id) = claimed_agent(&database).await;
    let transaction =
        super::super::begin_current_agent_transaction(&database, tenant_id, agent_id, &session_id)
            .await
            .unwrap();
    let Database::Sqlite(pool) = &database else {
        panic!("expected SQLite database")
    };
    let mut competing = pool.acquire().await.unwrap();
    sqlx::query("PRAGMA busy_timeout = 1")
        .execute(&mut *competing)
        .await
        .unwrap();

    let competing_begin = sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *competing)
        .await;

    assert!(
        competing_begin.is_err(),
        "current-agent transaction must reserve the SQLite writer before printer access"
    );
    transaction.rollback().await.unwrap();
}

async fn exercise_exact_session_guards(database: Database, mutation_database: Database) {
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(mutation_database.clone());
    let persisted_printers = PrinterRepository::new(database.clone());
    let jobs = JobRepository::new(mutation_database.clone());
    let materials = MaterialRepository::new(mutation_database.clone());
    let mutation_agents = AgentRepository::new(mutation_database);
    let tenant = tenants
        .create("session-guards", "Session Guards")
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let session_a = uuid::Uuid::new_v4().to_string();
    let session_b = uuid::Uuid::new_v4().to_string();
    agents
        .claim_online_session(
            tenant.id,
            agent.id,
            &session_a,
            "test-a",
            "2026-07-10T00:00:00Z",
        )
        .await
        .unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    let printer_before = persisted_printers
        .list_with_live_status_for_tenant(tenant.id)
        .await
        .unwrap()
        .into_iter()
        .find(|printer| printer.printer.id == printer_id)
        .unwrap();
    agents
        .claim_online_session(
            tenant.id,
            agent.id,
            &session_b,
            "test-b",
            "2026-07-10T00:01:00Z",
        )
        .await
        .unwrap();

    let heartbeat = mutation_agents
        .heartbeat_if_current(tenant.id, agent.id, &session_a, "2026-07-10T00:02:00Z")
        .await
        .unwrap_err();
    assert!(matches!(heartbeat, RepositoryError::AgentSessionNotCurrent));
    assert!(
        mutation_agents
            .mark_offline_if_current(tenant.id, agent.id, &session_a, "2026-07-10T00:02:00Z",)
            .await
            .unwrap()
            .is_none()
    );
    let snapshot = printers
        .upsert_snapshot_if_current(
            tenant.id,
            agent.id,
            &session_a,
            PrinterSnapshotUpsert {
                serial_number: format!("serial-{printer_id}"),
                host: None,
                access_code: None,
                name: "stale".to_string(),
                model: None,
                status: "stale".to_string(),
                observed_at: "2026-07-10T00:02:00Z".to_string(),
                nozzle_temperatures: Vec::new(),
                active_nozzle: None,
                bed_temperature_celsius: None,
                bed_target_temperature_celsius: None,
                chamber_temperature_celsius: None,
                chamber_light_on: None,
                connection_authoritative: false,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(snapshot, RepositoryError::AgentSessionNotCurrent));
    let report = jobs
        .apply_current_print_report(
            &session_a,
            super::jobs::report_input(tenant.id, agent.id, &printer_id, None, None, "RUNNING"),
        )
        .await
        .unwrap_err();
    assert!(matches!(report, RepositoryError::AgentSessionNotCurrent));
    let material = materials
        .apply_snapshot_if_current(
            &session_a,
            tenant.id,
            agent.id,
            &printer_id,
            format!("serial-{printer_id}"),
            "{}".to_string(),
        )
        .await
        .unwrap_err();
    assert!(matches!(material, RepositoryError::AgentSessionNotCurrent));

    let printer_after = persisted_printers
        .list_with_live_status_for_tenant(tenant.id)
        .await
        .unwrap()
        .into_iter()
        .find(|printer| printer.printer.id == printer_id)
        .unwrap();
    assert_eq!(printer_after, printer_before);
    assert!(
        MaterialRepository::new(database.clone())
            .latest_for_printer(tenant.id, &printer_id)
            .await
            .unwrap()
            .is_none()
    );

    let persisted = agent_entities::Entity::find_by_id(agent.id.to_string())
        .one(&database.sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        persisted.current_session_id.as_deref(),
        Some(session_b.as_str())
    );
    assert_eq!(persisted.status, "online");

    mutation_agents
        .heartbeat_if_current(tenant.id, agent.id, &session_b, "2026-07-10T00:03:00Z")
        .await
        .unwrap();
    let persisted = agent_entities::Entity::find_by_id(agent.id.to_string())
        .one(&database.sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        persisted.last_seen_at.as_deref(),
        Some("2026-07-10T00:03:00Z")
    );
    assert_eq!(
        persisted.current_session_id.as_deref(),
        Some(session_b.as_str())
    );
    assert!(
        mutation_agents
            .mark_offline_if_current(tenant.id, agent.id, &session_b, "2026-07-10T00:04:00Z",)
            .await
            .unwrap()
            .is_some()
    );
    let persisted = agent_entities::Entity::find_by_id(agent.id.to_string())
        .one(&database.sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.status, "offline");
    assert_eq!(persisted.current_session_id, None);
}

#[derive(Debug, Clone, Copy)]
enum CurrentAgentMutation {
    Snapshot,
    PrintReport,
    MaterialSnapshot,
}

#[derive(Serialize)]
struct CurrentMaterialPatchFixture {
    #[serde(rename = "type")]
    kind: &'static str,
    observed_at: &'static str,
}

async fn assert_postgres_agent_then_printer_lock_order(
    database: &Database,
    mutation_kind: CurrentAgentMutation,
) {
    let (tenant_id, agent_id, session_id) = claimed_agent(database).await;
    let printer_id = insert_printer_fixture(database, tenant_id, agent_id)
        .await
        .unwrap();
    let Database::Postgres(pool) = database else {
        panic!("expected PostgreSQL database")
    };
    let mut printer_locker = pool.begin().await.unwrap();
    let printer_locker_pid = sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()")
        .fetch_one(&mut *printer_locker)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM printers WHERE id = $1 FOR UPDATE")
        .bind(&printer_id)
        .fetch_one(&mut *printer_locker)
        .await
        .unwrap();

    let mut transaction_pause = super::super::current_transaction_pause::install(&session_id);
    let mutation_database = database.clone();
    let serial_number = format!("serial-{printer_id}");
    let mutation_printer_id = printer_id.clone();
    let mutation = tokio::spawn(async move {
        match mutation_kind {
            CurrentAgentMutation::Snapshot => PrinterRepository::new(mutation_database)
                .upsert_snapshot_if_current(
                    tenant_id,
                    agent_id,
                    &session_id,
                    PrinterSnapshotUpsert {
                        serial_number,
                        host: None,
                        access_code: None,
                        name: "locked snapshot".to_string(),
                        model: None,
                        status: "printing".to_string(),
                        observed_at: "2026-07-10T00:02:00Z".to_string(),
                        nozzle_temperatures: Vec::new(),
                        active_nozzle: None,
                        bed_temperature_celsius: None,
                        bed_target_temperature_celsius: None,
                        chamber_temperature_celsius: None,
                        chamber_light_on: None,
                        connection_authoritative: false,
                    },
                )
                .await
                .map(|_| ()),
            CurrentAgentMutation::PrintReport => JobRepository::new(mutation_database)
                .apply_current_print_report(
                    &session_id,
                    super::jobs::report_input(
                        tenant_id,
                        agent_id,
                        &mutation_printer_id,
                        None,
                        None,
                        "RUNNING",
                    ),
                )
                .await
                .map(|_| ()),
            CurrentAgentMutation::MaterialSnapshot => MaterialRepository::new(mutation_database)
                .apply_snapshot_if_current(
                    &session_id,
                    tenant_id,
                    agent_id,
                    &mutation_printer_id,
                    serial_number,
                    serde_json::to_string(&CurrentMaterialPatchFixture {
                        kind: "printer_material_patch",
                        observed_at: "2026-07-10T00:02:00Z",
                    })
                    .unwrap(),
                )
                .await
                .map(|_| ()),
        }
    });
    let mutation_pid = transaction_pause.wait_until_reached().await.unwrap();

    let mut replacement_connection = pool.acquire().await.unwrap();
    let replacement_pid = sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()")
        .fetch_one(&mut *replacement_connection)
        .await
        .unwrap();
    let agent_id = agent_id.to_string();
    let replacement = tokio::spawn(async move {
        sqlx::query("SELECT id FROM agents WHERE id = $1 FOR UPDATE")
            .bind(agent_id)
            .fetch_one(&mut *replacement_connection)
            .await
    });
    wait_until_postgres_blocked_by(pool, replacement_pid, mutation_pid).await;

    transaction_pause.resume();
    wait_until_postgres_blocked_by(pool, mutation_pid, printer_locker_pid).await;
    assert!(postgres_is_blocked_by(pool, replacement_pid, mutation_pid).await);

    printer_locker.rollback().await.unwrap();
    mutation.await.unwrap().unwrap();
    replacement.await.unwrap().unwrap();

    match mutation_kind {
        CurrentAgentMutation::Snapshot => {
            let printer = PrinterRepository::new(database.clone())
                .list_with_live_status_for_tenant(tenant_id)
                .await
                .unwrap()
                .into_iter()
                .find(|printer| printer.printer.id == printer_id)
                .unwrap();
            assert_eq!(printer.printer.status, "printing");
        }
        CurrentAgentMutation::PrintReport => {
            let printer = PrinterRepository::new(database.clone())
                .list_with_live_status_for_tenant(tenant_id)
                .await
                .unwrap()
                .into_iter()
                .find(|printer| printer.printer.id == printer_id)
                .unwrap();
            assert_eq!(printer.live_status.gcode_state.as_deref(), Some("RUNNING"));
        }
        CurrentAgentMutation::MaterialSnapshot => {
            let snapshot = MaterialRepository::new(database.clone())
                .latest_for_printer(tenant_id, &printer_id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(snapshot.observed_at, "2026-07-10T00:02:00Z");
        }
    }
}

async fn wait_until_postgres_blocked_by(pool: &sqlx::PgPool, blocked_pid: i32, blocker_pid: i32) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if postgres_is_blocked_by(pool, blocked_pid, blocker_pid).await {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("timed out waiting for PostgreSQL lock dependency");
}

async fn postgres_is_blocked_by(pool: &sqlx::PgPool, blocked_pid: i32, blocker_pid: i32) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT $2 = ANY(pg_blocking_pids($1))")
        .bind(blocked_pid)
        .bind(blocker_pid)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn claimed_agent(database: &Database) -> (TenantId, AgentId, String) {
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let tenant_slug = format!("locked-agent-{}", uuid::Uuid::new_v4());
    let tenant = tenants.create(&tenant_slug, "Locked Agent").await.unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let session_id = uuid::Uuid::new_v4().to_string();
    agents
        .claim_online_session(
            tenant.id,
            agent.id,
            &session_id,
            "test",
            "2026-07-10T00:00:00Z",
        )
        .await
        .unwrap();
    (tenant.id, agent.id, session_id)
}
