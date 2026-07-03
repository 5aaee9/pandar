use anyhow::Context;
use pandar_core::{AgentId, TenantId};

use crate::{
    db::Database,
    repositories::{PrinterSnapshotUpsert, RepositoryResult},
};

// SeaORM's generic update path is select-then-write here; keep one SQL escape hatch so
// SQLite and Postgres both preserve atomic ON CONFLICT upsert semantics for snapshots.
pub(crate) async fn upsert_snapshot(
    database: &Database,
    tenant_id: TenantId,
    agent_id: AgentId,
    printer_id: &str,
    snapshot: &PrinterSnapshotUpsert,
) -> RepositoryResult<()> {
    match database {
        Database::Sqlite(pool) => {
            let nozzle_temperatures_json = serde_json::to_string(&snapshot.nozzle_temperatures)
                .context("failed to serialize nozzle temperatures")?;
            sqlx::query(
                "INSERT INTO printers (
                     id, tenant_id, agent_id, serial_number, name, model, status,
                     last_seen_at, created_at, nozzle_temperatures_json,
                     active_nozzle, bed_temperature_celsius, bed_target_temperature_celsius,
                     chamber_temperature_celsius
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT (tenant_id, serial_number) DO UPDATE SET
                     agent_id = excluded.agent_id,
                     model = excluded.model,
                     status = excluded.status,
                     last_seen_at = excluded.last_seen_at,
                     nozzle_temperatures_json = excluded.nozzle_temperatures_json,
                     active_nozzle = excluded.active_nozzle,
                     bed_temperature_celsius = excluded.bed_temperature_celsius,
                     bed_target_temperature_celsius = excluded.bed_target_temperature_celsius,
                     chamber_temperature_celsius = excluded.chamber_temperature_celsius",
            )
            .bind(printer_id)
            .bind(tenant_id.to_string())
            .bind(agent_id.to_string())
            .bind(&snapshot.serial_number)
            .bind(&snapshot.name)
            .bind(&snapshot.model)
            .bind(&snapshot.status)
            .bind(&snapshot.observed_at)
            .bind(&nozzle_temperatures_json)
            .bind(&snapshot.active_nozzle)
            .bind(&snapshot.bed_temperature_celsius)
            .bind(&snapshot.bed_target_temperature_celsius)
            .bind(&snapshot.chamber_temperature_celsius)
            .execute(pool)
            .await
            .context("failed to upsert SQLite printer snapshot")?;
        }
        Database::Postgres(pool) => {
            let nozzle_temperatures_json = serde_json::to_string(&snapshot.nozzle_temperatures)
                .context("failed to serialize nozzle temperatures")?;
            sqlx::query(
                "INSERT INTO printers (
                     id, tenant_id, agent_id, serial_number, name, model, status,
                     last_seen_at, created_at, nozzle_temperatures_json,
                     active_nozzle, bed_temperature_celsius, bed_target_temperature_celsius,
                     chamber_temperature_celsius
                 )
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8, $9, $10, $11, $12, $13)
                 ON CONFLICT (tenant_id, serial_number) DO UPDATE SET
                     agent_id = excluded.agent_id,
                     model = excluded.model,
                     status = excluded.status,
                     last_seen_at = excluded.last_seen_at,
                     nozzle_temperatures_json = excluded.nozzle_temperatures_json,
                     active_nozzle = excluded.active_nozzle,
                     bed_temperature_celsius = excluded.bed_temperature_celsius,
                     bed_target_temperature_celsius = excluded.bed_target_temperature_celsius,
                     chamber_temperature_celsius = excluded.chamber_temperature_celsius",
            )
            .bind(printer_id)
            .bind(tenant_id.to_string())
            .bind(agent_id.to_string())
            .bind(&snapshot.serial_number)
            .bind(&snapshot.name)
            .bind(&snapshot.model)
            .bind(&snapshot.status)
            .bind(&snapshot.observed_at)
            .bind(&nozzle_temperatures_json)
            .bind(&snapshot.active_nozzle)
            .bind(&snapshot.bed_temperature_celsius)
            .bind(&snapshot.bed_target_temperature_celsius)
            .bind(&snapshot.chamber_temperature_celsius)
            .execute(pool)
            .await
            .context("failed to upsert PostgreSQL printer snapshot")?;
        }
    }

    Ok(())
}
