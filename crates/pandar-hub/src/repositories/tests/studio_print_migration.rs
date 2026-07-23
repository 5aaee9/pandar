use std::str::FromStr;

use pandar_core::StudioPrintMetadata;
use serde::{Deserialize, Serialize};
use sqlx::{
    migrate::Migrator,
    postgres::{PgConnectOptions, PgPoolOptions},
};

use super::*;
use crate::repositories::test_helpers::insert_printer_fixture;

const PREVIOUS_MIGRATION: i64 = 20260720000000;
static SQLITE_MIGRATOR: Migrator = sqlx::migrate!("migrations/sqlite");
static POSTGRES_MIGRATOR: Migrator = sqlx::migrate!("migrations/postgres");

#[tokio::test]
async fn sqlite_studio_print_contract_migration_backfills_stable_tenant_ids() {
    let config = DatabaseConfig::from_url("sqlite::memory:").unwrap();
    let database = Database::connect(&config).await.unwrap();
    exercise_studio_print_migration(database).await;
}

#[tokio::test]
async fn postgres_studio_print_contract_migration_backfills_stable_tenant_ids_when_configured() {
    let Ok(url) = std::env::var("PANDAR_TEST_POSTGRES_URL") else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let schema = format!("pandar_studio_print_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let options = PgConnectOptions::from_str(&url)
        .unwrap()
        .options([("search_path", schema.as_str())]);
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let database = Database::Postgres(pool);

    exercise_studio_print_migration(database.clone()).await;

    let Database::Postgres(pool) = database else {
        unreachable!();
    };
    pool.close().await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

async fn exercise_studio_print_migration(database: Database) {
    match &database {
        Database::Sqlite(pool) => SQLITE_MIGRATOR
            .run_to(PREVIOUS_MIGRATION, pool)
            .await
            .unwrap(),
        Database::Postgres(pool) => POSTGRES_MIGRATOR
            .run_to(PREVIOUS_MIGRATION, pool)
            .await
            .unwrap(),
    }
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let first_tenant = tenants
        .create(&format!("migration-a-{}", uuid::Uuid::new_v4()), "A")
        .await
        .unwrap();
    let second_tenant = tenants
        .create(&format!("migration-b-{}", uuid::Uuid::new_v4()), "B")
        .await
        .unwrap();
    let first_agent = agents.create(first_tenant.id, "agent").await.unwrap();
    let second_agent = agents.create(second_tenant.id, "agent").await.unwrap();
    let first_printer = insert_printer_fixture(&database, first_tenant.id, first_agent.id)
        .await
        .unwrap();
    let second_printer = insert_printer_fixture(&database, second_tenant.id, second_agent.id)
        .await
        .unwrap();
    insert_legacy_job(
        &database,
        first_tenant.id,
        first_agent.id,
        &first_printer,
        "00000000-0000-4000-8000-000000000002",
        "2026-07-20T00:00:00.11Z",
        7,
    )
    .await;
    insert_legacy_job(
        &database,
        first_tenant.id,
        first_agent.id,
        &first_printer,
        "00000000-0000-4000-8000-000000000001",
        "2026-07-20T00:00:00.1Z",
        3,
    )
    .await;
    insert_legacy_job(
        &database,
        second_tenant.id,
        second_agent.id,
        &second_printer,
        "00000000-0000-4000-8000-000000000003",
        "2026-07-20T00:00:00Z",
        9,
    )
    .await;

    match &database {
        Database::Sqlite(pool) => SQLITE_MIGRATOR.run(pool).await.unwrap(),
        Database::Postgres(pool) => POSTGRES_MIGRATOR.run(pool).await.unwrap(),
    }

    let first = load_ids(&database, first_tenant.id).await;
    let second = load_ids(&database, second_tenant.id).await;
    assert_eq!(
        first,
        vec![
            ("00000000-0000-4000-8000-000000000001".to_owned(), 1),
            ("00000000-0000-4000-8000-000000000002".to_owned(), 2)
        ]
    );
    assert_eq!(
        second,
        vec![("00000000-0000-4000-8000-000000000003".to_owned(), 1)]
    );
    assert_eq!(
        load_plate_indices(&database, first_tenant.id).await,
        vec![3, 7]
    );
    assert_eq!(
        load_plate_indices(&database, second_tenant.id).await,
        vec![9]
    );
    assert_eq!(
        load_sequence(&database, first_tenant.id).await,
        2,
        "backfill advances the tenant sequence"
    );
    assert_eq!(load_sequence(&database, second_tenant.id).await, 1);

    let first_payloads = load_command_payloads(&database, first_tenant.id).await;
    assert_eq!(
        first_payloads
            .iter()
            .map(|payload| payload.studio_submission_id)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        first_payloads
            .iter()
            .map(|payload| payload.plate_id)
            .collect::<Vec<_>>(),
        vec![3, 7]
    );
    assert!(
        first_payloads
            .iter()
            .all(|payload| payload.studio_metadata.is_none())
    );
    let second_payloads = load_command_payloads(&database, second_tenant.id).await;
    assert_eq!(second_payloads[0].studio_submission_id, 1);
    assert!(second_payloads[0].studio_metadata.is_none());
}

async fn insert_legacy_job(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
    printer_id: &str,
    job_id: &str,
    created_at: &str,
    plate_id: u32,
) {
    let artifact_id = format!("artifact-{job_id}");
    let command_id = format!("10000000-0000-4000-8000-{}", &job_id[24..]);
    let payload_json = serde_json::to_string(&LegacyPrintProjectFilePayload { plate_id }).unwrap();
    match database {
        Database::Sqlite(pool) => {
            sqlx::query("INSERT INTO job_artifacts (id, tenant_id, filename, content_type, size_bytes, storage_path, created_at) VALUES (?1, ?2, 'plate.3mf', 'model/3mf', 42, ?3, ?4)")
                .bind(&artifact_id).bind(tenant_id.to_string()).bind(format!("{tenant_id}/{artifact_id}/plate.3mf")).bind(created_at).execute(pool).await.unwrap();
            sqlx::query("INSERT INTO commands (id, tenant_id, agent_id, printer_id, kind, status, payload_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'print_project_file', 'queued', ?5, ?6, ?6)")
                .bind(&command_id).bind(tenant_id.to_string()).bind(agent_id.to_string()).bind(printer_id).bind(&payload_json).bind(created_at).execute(pool).await.unwrap();
            sqlx::query("INSERT INTO jobs (id, tenant_id, printer_id, agent_id, artifact_id, command_id, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'queued', ?7, ?7)")
                .bind(job_id).bind(tenant_id.to_string()).bind(printer_id).bind(agent_id.to_string()).bind(&artifact_id).bind(&command_id).bind(created_at).execute(pool).await.unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::query("INSERT INTO job_artifacts (id, tenant_id, filename, content_type, size_bytes, storage_path, created_at) VALUES ($1, $2, 'plate.3mf', 'model/3mf', 42, $3, $4)")
                .bind(&artifact_id).bind(tenant_id.to_string()).bind(format!("{tenant_id}/{artifact_id}/plate.3mf")).bind(created_at).execute(pool).await.unwrap();
            sqlx::query("INSERT INTO commands (id, tenant_id, agent_id, printer_id, kind, status, payload_json, created_at, updated_at) VALUES ($1, $2, $3, $4, 'print_project_file', 'queued', $5, $6, $6)")
                .bind(&command_id).bind(tenant_id.to_string()).bind(agent_id.to_string()).bind(printer_id).bind(&payload_json).bind(created_at).execute(pool).await.unwrap();
            sqlx::query("INSERT INTO jobs (id, tenant_id, printer_id, agent_id, artifact_id, command_id, status, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, 'queued', $7, $7)")
                .bind(job_id).bind(tenant_id.to_string()).bind(printer_id).bind(agent_id.to_string()).bind(&artifact_id).bind(&command_id).bind(created_at).execute(pool).await.unwrap();
        }
    }
}

#[derive(Serialize)]
struct LegacyPrintProjectFilePayload {
    plate_id: u32,
}

async fn load_ids(database: &Database, tenant_id: TenantId) -> Vec<(String, i32)> {
    match database {
        Database::Sqlite(pool) => sqlx::query_as("SELECT id, studio_submission_id FROM jobs WHERE tenant_id = ?1 ORDER BY studio_submission_id")
            .bind(tenant_id.to_string()).fetch_all(pool).await.unwrap(),
        Database::Postgres(pool) => sqlx::query_as("SELECT id, studio_submission_id FROM jobs WHERE tenant_id = $1 ORDER BY studio_submission_id")
            .bind(tenant_id.to_string()).fetch_all(pool).await.unwrap(),
    }
}

async fn load_sequence(database: &Database, tenant_id: TenantId) -> i32 {
    match database {
        Database::Sqlite(pool) => sqlx::query_scalar(
            "SELECT last_id FROM studio_submission_sequences WHERE tenant_id = ?1",
        )
        .bind(tenant_id.to_string())
        .fetch_one(pool)
        .await
        .unwrap(),
        Database::Postgres(pool) => sqlx::query_scalar(
            "SELECT last_id FROM studio_submission_sequences WHERE tenant_id = $1",
        )
        .bind(tenant_id.to_string())
        .fetch_one(pool)
        .await
        .unwrap(),
    }
}

async fn load_plate_indices(database: &Database, tenant_id: TenantId) -> Vec<i32> {
    match database {
        Database::Sqlite(pool) => sqlx::query_scalar(
            "SELECT plate_index FROM jobs WHERE tenant_id = ?1 ORDER BY studio_submission_id",
        )
        .bind(tenant_id.to_string())
        .fetch_all(pool)
        .await
        .unwrap(),
        Database::Postgres(pool) => sqlx::query_scalar(
            "SELECT plate_index FROM jobs WHERE tenant_id = $1 ORDER BY studio_submission_id",
        )
        .bind(tenant_id.to_string())
        .fetch_all(pool)
        .await
        .unwrap(),
    }
}

#[derive(Deserialize)]
struct MigratedStudioCommandFields {
    plate_id: u32,
    studio_submission_id: i32,
    studio_metadata: Option<StudioPrintMetadata>,
}

async fn load_command_payloads(
    database: &Database,
    tenant_id: TenantId,
) -> Vec<MigratedStudioCommandFields> {
    let payloads: Vec<String> = match database {
        Database::Sqlite(pool) => sqlx::query_scalar(
            "SELECT commands.payload_json FROM commands JOIN jobs ON jobs.command_id = commands.id WHERE jobs.tenant_id = ?1 ORDER BY jobs.studio_submission_id",
        )
        .bind(tenant_id.to_string())
        .fetch_all(pool)
        .await
        .unwrap(),
        Database::Postgres(pool) => sqlx::query_scalar(
            "SELECT commands.payload_json FROM commands JOIN jobs ON jobs.command_id = commands.id WHERE jobs.tenant_id = $1 ORDER BY jobs.studio_submission_id",
        )
        .bind(tenant_id.to_string())
        .fetch_all(pool)
        .await
        .unwrap(),
    };
    payloads
        .into_iter()
        .map(|payload| serde_json::from_str(&payload).unwrap())
        .collect()
}
