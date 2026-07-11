use anyhow::Context;
use pandar_core::{AgentId, BambuDeviceFeatures, TenantId};
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseTransaction, Statement};

use crate::repositories::{PrinterSnapshotUpsert, RepositoryResult};

// SeaORM's generic update path is select-then-write here; keep one SQL escape hatch so
// SQLite and Postgres both preserve atomic ON CONFLICT upsert semantics for snapshots.
pub(crate) async fn upsert_snapshot(
    transaction: &DatabaseTransaction,
    tenant_id: TenantId,
    agent_id: AgentId,
    printer_id: &str,
    snapshot: &PrinterSnapshotUpsert,
    device_features: Option<BambuDeviceFeatures>,
    device_features_session_id: Option<&str>,
) -> RepositoryResult<()> {
    let bambu_fun_bits = device_features.map(BambuDeviceFeatures::to_hex);
    let bambu_fun_session_id = device_features_session_id.map(str::to_owned);
    match transaction.get_database_backend() {
        DatabaseBackend::Sqlite => {
            let nozzle_temperatures_json = serde_json::to_string(&snapshot.nozzle_temperatures)
                .context("failed to serialize nozzle temperatures")?;
            transaction
                .execute_raw(Statement::from_sql_and_values(
                    DatabaseBackend::Sqlite,
                    "INSERT INTO printers (
                     id, tenant_id, agent_id, serial_number, host, access_code, name, model, status,
                     last_seen_at, created_at, nozzle_temperatures_json,
                     active_nozzle, bed_temperature_celsius, bed_target_temperature_celsius,
                     chamber_temperature_celsius, chamber_light_on, bambu_fun_bits,
                     bambu_fun_session_id, state_revision
                  )
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, 1)
                 ON CONFLICT (tenant_id, serial_number) DO UPDATE SET
                     agent_id = excluded.agent_id,
                     host = COALESCE(excluded.host, printers.host),
                     access_code = COALESCE(excluded.access_code, printers.access_code),
                     model = excluded.model,
                     status = excluded.status,
                     last_seen_at = excluded.last_seen_at,
                     nozzle_temperatures_json = excluded.nozzle_temperatures_json,
                     active_nozzle = excluded.active_nozzle,
                     bed_temperature_celsius = excluded.bed_temperature_celsius,
                     bed_target_temperature_celsius = excluded.bed_target_temperature_celsius,
                     chamber_temperature_celsius = excluded.chamber_temperature_celsius,
                     chamber_light_on = excluded.chamber_light_on,
                     bambu_fun_bits = COALESCE(excluded.bambu_fun_bits, printers.bambu_fun_bits),
                     bambu_fun_session_id = CASE
                         WHEN excluded.bambu_fun_bits IS NULL THEN printers.bambu_fun_session_id
                         ELSE excluded.bambu_fun_session_id
                     END,
                     state_revision = printers.state_revision + 1",
                    vec![
                        printer_id.to_owned().into(),
                        tenant_id.to_string().into(),
                        agent_id.to_string().into(),
                        snapshot.serial_number.clone().into(),
                        snapshot.host.clone().into(),
                        snapshot.access_code.clone().into(),
                        snapshot.name.clone().into(),
                        snapshot.model.clone().into(),
                        snapshot.status.clone().into(),
                        snapshot.observed_at.clone().into(),
                        nozzle_temperatures_json.into(),
                        snapshot.active_nozzle.clone().into(),
                        snapshot.bed_temperature_celsius.clone().into(),
                        snapshot.bed_target_temperature_celsius.clone().into(),
                        snapshot.chamber_temperature_celsius.clone().into(),
                        snapshot.chamber_light_on.into(),
                        bambu_fun_bits.clone().into(),
                        bambu_fun_session_id.clone().into(),
                    ],
                ))
                .await
                .context("failed to upsert SQLite printer snapshot")?;
        }
        DatabaseBackend::Postgres => {
            let nozzle_temperatures_json = serde_json::to_string(&snapshot.nozzle_temperatures)
                .context("failed to serialize nozzle temperatures")?;
            transaction
                .execute_raw(Statement::from_sql_and_values(
                    DatabaseBackend::Postgres,
                    "INSERT INTO printers (
                     id, tenant_id, agent_id, serial_number, host, access_code, name, model, status,
                     last_seen_at, created_at, nozzle_temperatures_json,
                     active_nozzle, bed_temperature_celsius, bed_target_temperature_celsius,
                     chamber_temperature_celsius, chamber_light_on, bambu_fun_bits,
                     bambu_fun_session_id, state_revision
                  )
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10, $11, $12, $13, $14, $15, $16, $17, $18, 1)
                 ON CONFLICT (tenant_id, serial_number) DO UPDATE SET
                     agent_id = excluded.agent_id,
                     host = COALESCE(excluded.host, printers.host),
                     access_code = COALESCE(excluded.access_code, printers.access_code),
                     model = excluded.model,
                     status = excluded.status,
                     last_seen_at = excluded.last_seen_at,
                     nozzle_temperatures_json = excluded.nozzle_temperatures_json,
                     active_nozzle = excluded.active_nozzle,
                     bed_temperature_celsius = excluded.bed_temperature_celsius,
                     bed_target_temperature_celsius = excluded.bed_target_temperature_celsius,
                     chamber_temperature_celsius = excluded.chamber_temperature_celsius,
                     chamber_light_on = excluded.chamber_light_on,
                     bambu_fun_bits = COALESCE(excluded.bambu_fun_bits, printers.bambu_fun_bits),
                     bambu_fun_session_id = CASE
                         WHEN excluded.bambu_fun_bits IS NULL THEN printers.bambu_fun_session_id
                         ELSE excluded.bambu_fun_session_id
                     END,
                     state_revision = printers.state_revision + 1",
                    vec![
                        printer_id.to_owned().into(),
                        tenant_id.to_string().into(),
                        agent_id.to_string().into(),
                        snapshot.serial_number.clone().into(),
                        snapshot.host.clone().into(),
                        snapshot.access_code.clone().into(),
                        snapshot.name.clone().into(),
                        snapshot.model.clone().into(),
                        snapshot.status.clone().into(),
                        snapshot.observed_at.clone().into(),
                        nozzle_temperatures_json.into(),
                        snapshot.active_nozzle.clone().into(),
                        snapshot.bed_temperature_celsius.clone().into(),
                        snapshot.bed_target_temperature_celsius.clone().into(),
                        snapshot.chamber_temperature_celsius.clone().into(),
                        snapshot.chamber_light_on.into(),
                        bambu_fun_bits.into(),
                        bambu_fun_session_id.into(),
                    ],
                ))
                .await
                .context("failed to upsert PostgreSQL printer snapshot")?;
        }
        backend => unreachable!("unsupported printer snapshot backend {backend:?}"),
    }

    Ok(())
}
