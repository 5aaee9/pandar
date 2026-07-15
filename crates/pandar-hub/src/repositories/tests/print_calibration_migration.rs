use std::str::FromStr;

use pandar_core::PrintCalibrationMode;
use sqlx::{
    migrate::Migrator,
    postgres::{PgConnectOptions, PgPoolOptions},
};

use super::*;
use crate::repositories::PrintProjectFilePayload;

const PREVIOUS_MIGRATION: i64 = 20260711010000;
static SQLITE_MIGRATOR: Migrator = sqlx::migrate!("migrations/sqlite");
static POSTGRES_MIGRATOR: Migrator = sqlx::migrate!("migrations/postgres");

#[tokio::test]
async fn sqlite_backfills_legacy_print_calibration_payloads() {
    let config = DatabaseConfig::from_url("sqlite::memory:").unwrap();
    let database = Database::connect(&config).await.unwrap();

    exercise_print_calibration_migration(database).await;
}

#[tokio::test]
async fn postgres_backfills_legacy_print_calibration_payloads_when_configured() {
    let Ok(url) = std::env::var("PANDAR_TEST_POSTGRES_URL") else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let schema = format!("pandar_print_calibration_{}", uuid::Uuid::new_v4().simple());
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

    exercise_print_calibration_migration(database.clone()).await;

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

async fn exercise_print_calibration_migration(database: Database) {
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
    let tenant = tenants
        .create(
            &format!("print-calibration-{}", uuid::Uuid::new_v4()),
            "Print Calibration",
        )
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "migration-agent").await.unwrap();
    let legacy = legacy_payload(true);
    let mut partial = legacy_payload(false);
    {
        let partial = partial.as_object_mut().unwrap();
        partial.insert("bed_leveling".to_owned(), serde_json::json!(true));
        partial.insert("auto_bed_leveling".to_owned(), serde_json::json!(2));
        partial.insert("auto_flow_cali".to_owned(), serde_json::json!(2));
        partial.insert("auto_offset_cali".to_owned(), serde_json::json!(1));
    }
    insert_command(
        &database,
        tenant.id,
        agent.id,
        "legacy-calibration",
        &legacy.to_string(),
    )
    .await;
    insert_command(
        &database,
        tenant.id,
        agent.id,
        "partial-calibration",
        &partial.to_string(),
    )
    .await;

    match &database {
        Database::Sqlite(pool) => SQLITE_MIGRATOR.run(pool).await.unwrap(),
        Database::Postgres(pool) => POSTGRES_MIGRATOR.run(pool).await.unwrap(),
    }

    let legacy: PrintProjectFilePayload =
        serde_json::from_str(&load_payload(&database, "legacy-calibration").await).unwrap();
    assert!(!legacy.bed_leveling);
    assert_eq!(legacy.auto_bed_leveling, PrintCalibrationMode::Off);
    assert!(legacy.flow_cali);
    assert_eq!(legacy.auto_flow_cali, PrintCalibrationMode::On);
    assert_eq!(legacy.auto_offset_cali, PrintCalibrationMode::Off);

    let partial: PrintProjectFilePayload =
        serde_json::from_str(&load_payload(&database, "partial-calibration").await).unwrap();
    assert!(partial.bed_leveling);
    assert_eq!(partial.auto_bed_leveling, PrintCalibrationMode::Auto);
    assert!(!partial.flow_cali);
    assert_eq!(partial.auto_flow_cali, PrintCalibrationMode::Auto);
    assert_eq!(partial.auto_offset_cali, PrintCalibrationMode::On);
}

fn legacy_payload(flow_cali: bool) -> serde_json::Value {
    serde_json::json!({
        "job_id": "job",
        "artifact_id": "artifact",
        "printer_id": "printer",
        "serial_number": "serial",
        "filename": "plate.3mf",
        "storage_path": "tenant/artifact/plate.3mf",
        "artifact_download_path": "/api/v1/agents/agent/artifacts/artifact",
        "size_bytes": 42,
        "plate_id": 1,
        "use_ams": true,
        "flow_cali": flow_cali,
        "timelapse": false,
        "ams_mapping_json": null,
        "ams_mapping2_json": null,
        "ams_mapping_info_json": null
    })
}

async fn insert_command(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
    id: &str,
    payload: &str,
) {
    let now = "2026-07-15T00:00:00Z";
    match database {
        Database::Sqlite(pool) => {
            sqlx::query(
                "INSERT INTO commands (id, tenant_id, agent_id, printer_id, kind, status, payload_json, error, created_at, updated_at)
                 VALUES (?1, ?2, ?3, NULL, 'print_project_file', 'queued', ?4, NULL, ?5, ?5)",
            )
            .bind(id)
            .bind(tenant_id.to_string())
            .bind(agent_id.to_string())
            .bind(payload)
            .bind(now)
            .execute(pool)
            .await
            .unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO commands (id, tenant_id, agent_id, printer_id, kind, status, payload_json, error, created_at, updated_at)
                 VALUES ($1, $2, $3, NULL, 'print_project_file', 'queued', $4, NULL, $5, $5)",
            )
            .bind(id)
            .bind(tenant_id.to_string())
            .bind(agent_id.to_string())
            .bind(payload)
            .bind(now)
            .execute(pool)
            .await
            .unwrap();
        }
    }
}

async fn load_payload(database: &Database, id: &str) -> String {
    match database {
        Database::Sqlite(pool) => {
            sqlx::query_scalar("SELECT payload_json FROM commands WHERE id = ?1")
                .bind(id)
                .fetch_one(pool)
                .await
                .unwrap()
        }
        Database::Postgres(pool) => {
            sqlx::query_scalar("SELECT payload_json FROM commands WHERE id = $1")
                .bind(id)
                .fetch_one(pool)
                .await
                .unwrap()
        }
    }
}
