use anyhow::Context;
use pandar_core::{AgentId, BambuDeviceFeatures, TenantId};
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseTransaction, Statement};

use crate::repositories::{
    PrinterSnapshotUpsert, RepositoryResult, printers::SnapshotSessionState,
};

struct TelemetryWriteMask {
    status: bool,
    nozzle_temperatures: bool,
    active_nozzle: bool,
    bed_temperature: bool,
    bed_target_temperature: bool,
    chamber_temperature: bool,
    chamber_target_temperature: bool,
    chamber_light: bool,
}

impl TelemetryWriteMask {
    fn from_snapshot(snapshot: &PrinterSnapshotUpsert) -> Self {
        let authoritative = snapshot.telemetry_authoritative;
        Self {
            status: authoritative || snapshot.status.is_some(),
            nozzle_temperatures: authoritative || !snapshot.nozzle_temperatures.is_empty(),
            active_nozzle: authoritative || snapshot.active_nozzle.is_some(),
            bed_temperature: authoritative || snapshot.bed_temperature_celsius.is_some(),
            bed_target_temperature: authoritative
                || snapshot.bed_target_temperature_celsius.is_some(),
            chamber_temperature: authoritative || snapshot.chamber_temperature_celsius.is_some(),
            chamber_target_temperature: authoritative
                || snapshot.chamber_target_temperature_celsius.is_some(),
            chamber_light: authoritative || snapshot.chamber_light_on.is_some(),
        }
    }
}

// SeaORM's generic update path is select-then-write here; keep one SQL escape hatch so
// SQLite and Postgres both preserve atomic ON CONFLICT upsert semantics for snapshots.
pub(crate) async fn upsert_snapshot(
    transaction: &DatabaseTransaction,
    tenant_id: TenantId,
    agent_id: AgentId,
    snapshot: &PrinterSnapshotUpsert,
    access_code_encrypted: Option<&str>,
    session_state: SnapshotSessionState<'_>,
) -> RepositoryResult<()> {
    let printer_id = uuid::Uuid::new_v4().to_string();
    let bambu_fun_bits = session_state
        .device_features
        .map(BambuDeviceFeatures::to_hex);
    let bambu_fun_session_id = session_state.device_features_session_id.map(str::to_owned);
    let bambu_nozzle_system_json = snapshot
        .nozzle_system
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .context("failed to serialize Bambu nozzle system")?;
    let bambu_nozzle_system_session_id = session_state.nozzle_system_session_id.map(str::to_owned);
    let mqtt_presence_session_id = session_state.mqtt_presence_session_id.map(str::to_owned);
    let telemetry_write = TelemetryWriteMask::from_snapshot(snapshot);
    let status = snapshot
        .status
        .clone()
        .unwrap_or_else(|| "unknown".to_owned());
    match transaction.get_database_backend() {
        DatabaseBackend::Sqlite => {
            let nozzle_temperatures_json = serde_json::to_string(&snapshot.nozzle_temperatures)
                .context("failed to serialize nozzle temperatures")?;
            transaction
                .execute_raw(Statement::from_sql_and_values(
                    DatabaseBackend::Sqlite,
                    "INSERT INTO printers (
                     id, tenant_id, agent_id, serial_number, host, access_code,
                     access_code_encrypted, name, model, status,
                     last_seen_at, created_at, nozzle_temperatures_json,
                     active_nozzle, bed_temperature_celsius, bed_target_temperature_celsius,
                     chamber_temperature_celsius, chamber_target_temperature_celsius,
                     chamber_light_on, bambu_fun_bits,
                     bambu_fun_session_id, bambu_nozzle_system_json,
                     bambu_nozzle_system_session_id, mqtt_presence_session_id, state_revision
                  )
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?8, ?9, ?10, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, 1)
                 ON CONFLICT (tenant_id, serial_number) DO UPDATE SET
                     agent_id = excluded.agent_id,
                     host = CASE
                         WHEN ?23 THEN excluded.host
                         ELSE COALESCE(printers.host, excluded.host)
                     END,
                     access_code = NULL,
                     access_code_encrypted = CASE
                         WHEN ?23 THEN excluded.access_code_encrypted
                         ELSE COALESCE(printers.access_code_encrypted, excluded.access_code_encrypted)
                     END,
                     model = COALESCE(excluded.model, printers.model),
                     status = CASE WHEN ?24 THEN excluded.status ELSE printers.status END,
                     last_seen_at = excluded.last_seen_at,
                     nozzle_temperatures_json = CASE WHEN ?25 THEN excluded.nozzle_temperatures_json ELSE printers.nozzle_temperatures_json END,
                     active_nozzle = CASE WHEN ?26 THEN excluded.active_nozzle ELSE printers.active_nozzle END,
                     bed_temperature_celsius = CASE WHEN ?27 THEN excluded.bed_temperature_celsius ELSE printers.bed_temperature_celsius END,
                     bed_target_temperature_celsius = CASE WHEN ?28 THEN excluded.bed_target_temperature_celsius ELSE printers.bed_target_temperature_celsius END,
                     chamber_temperature_celsius = CASE WHEN ?29 THEN excluded.chamber_temperature_celsius ELSE printers.chamber_temperature_celsius END,
                     chamber_target_temperature_celsius = CASE WHEN ?30 THEN excluded.chamber_target_temperature_celsius ELSE printers.chamber_target_temperature_celsius END,
                     chamber_light_on = CASE WHEN ?31 THEN excluded.chamber_light_on ELSE printers.chamber_light_on END,
                     bambu_fun_bits = COALESCE(excluded.bambu_fun_bits, printers.bambu_fun_bits),
                     bambu_fun_session_id = CASE
                         WHEN excluded.bambu_fun_bits IS NULL THEN printers.bambu_fun_session_id
                         ELSE excluded.bambu_fun_session_id
                     END,
                     bambu_nozzle_system_json = COALESCE(excluded.bambu_nozzle_system_json, printers.bambu_nozzle_system_json),
                     bambu_nozzle_system_session_id = CASE
                         WHEN excluded.bambu_nozzle_system_json IS NULL THEN printers.bambu_nozzle_system_session_id
                         ELSE excluded.bambu_nozzle_system_session_id
                     END,
                     mqtt_presence_session_id = COALESCE(excluded.mqtt_presence_session_id, printers.mqtt_presence_session_id),
                     state_revision = printers.state_revision + 1",
                    vec![
                        printer_id.clone().into(),
                        tenant_id.to_string().into(),
                        agent_id.to_string().into(),
                        snapshot.serial_number.clone().into(),
                        snapshot.host.clone().into(),
                        access_code_encrypted.map(str::to_owned).into(),
                        snapshot.name.clone().into(),
                        snapshot.model.clone().into(),
                        status.clone().into(),
                        snapshot.observed_at.clone().into(),
                        nozzle_temperatures_json.into(),
                        snapshot.active_nozzle.clone().into(),
                        snapshot.bed_temperature_celsius.clone().into(),
                        snapshot.bed_target_temperature_celsius.clone().into(),
                        snapshot.chamber_temperature_celsius.clone().into(),
                        snapshot.chamber_target_temperature_celsius.clone().into(),
                        snapshot.chamber_light_on.into(),
                        bambu_fun_bits.clone().into(),
                        bambu_fun_session_id.clone().into(),
                        bambu_nozzle_system_json.clone().into(),
                        bambu_nozzle_system_session_id.clone().into(),
                        mqtt_presence_session_id.clone().into(),
                        snapshot.connection_authoritative.into(),
                        telemetry_write.status.into(),
                        telemetry_write.nozzle_temperatures.into(),
                        telemetry_write.active_nozzle.into(),
                        telemetry_write.bed_temperature.into(),
                        telemetry_write.bed_target_temperature.into(),
                        telemetry_write.chamber_temperature.into(),
                        telemetry_write.chamber_target_temperature.into(),
                        telemetry_write.chamber_light.into(),
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
                     id, tenant_id, agent_id, serial_number, host, access_code,
                     access_code_encrypted, name, model, status,
                     last_seen_at, created_at, nozzle_temperatures_json,
                     active_nozzle, bed_temperature_celsius, bed_target_temperature_celsius,
                     chamber_temperature_celsius, chamber_target_temperature_celsius,
                     chamber_light_on, bambu_fun_bits,
                     bambu_fun_session_id, bambu_nozzle_system_json,
                     bambu_nozzle_system_session_id, mqtt_presence_session_id, state_revision
                  )
                 VALUES ($1, $2, $3, $4, $5, NULL, $6, $7, $8, $9, $10, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, 1)
                 ON CONFLICT (tenant_id, serial_number) DO UPDATE SET
                     agent_id = excluded.agent_id,
                     host = CASE
                         WHEN $23 THEN excluded.host
                         ELSE COALESCE(printers.host, excluded.host)
                     END,
                     access_code = NULL,
                     access_code_encrypted = CASE
                         WHEN $23 THEN excluded.access_code_encrypted
                         ELSE COALESCE(printers.access_code_encrypted, excluded.access_code_encrypted)
                     END,
                     model = COALESCE(excluded.model, printers.model),
                     status = CASE WHEN $24 THEN excluded.status ELSE printers.status END,
                     last_seen_at = excluded.last_seen_at,
                     nozzle_temperatures_json = CASE WHEN $25 THEN excluded.nozzle_temperatures_json ELSE printers.nozzle_temperatures_json END,
                     active_nozzle = CASE WHEN $26 THEN excluded.active_nozzle ELSE printers.active_nozzle END,
                     bed_temperature_celsius = CASE WHEN $27 THEN excluded.bed_temperature_celsius ELSE printers.bed_temperature_celsius END,
                     bed_target_temperature_celsius = CASE WHEN $28 THEN excluded.bed_target_temperature_celsius ELSE printers.bed_target_temperature_celsius END,
                     chamber_temperature_celsius = CASE WHEN $29 THEN excluded.chamber_temperature_celsius ELSE printers.chamber_temperature_celsius END,
                     chamber_target_temperature_celsius = CASE WHEN $30 THEN excluded.chamber_target_temperature_celsius ELSE printers.chamber_target_temperature_celsius END,
                     chamber_light_on = CASE WHEN $31 THEN excluded.chamber_light_on ELSE printers.chamber_light_on END,
                     bambu_fun_bits = COALESCE(excluded.bambu_fun_bits, printers.bambu_fun_bits),
                     bambu_fun_session_id = CASE
                         WHEN excluded.bambu_fun_bits IS NULL THEN printers.bambu_fun_session_id
                         ELSE excluded.bambu_fun_session_id
                     END,
                     bambu_nozzle_system_json = COALESCE(excluded.bambu_nozzle_system_json, printers.bambu_nozzle_system_json),
                     bambu_nozzle_system_session_id = CASE
                         WHEN excluded.bambu_nozzle_system_json IS NULL THEN printers.bambu_nozzle_system_session_id
                         ELSE excluded.bambu_nozzle_system_session_id
                     END,
                     mqtt_presence_session_id = COALESCE(excluded.mqtt_presence_session_id, printers.mqtt_presence_session_id),
                     state_revision = printers.state_revision + 1",
                    vec![
                        printer_id.into(),
                        tenant_id.to_string().into(),
                        agent_id.to_string().into(),
                        snapshot.serial_number.clone().into(),
                        snapshot.host.clone().into(),
                        access_code_encrypted.map(str::to_owned).into(),
                        snapshot.name.clone().into(),
                        snapshot.model.clone().into(),
                        status.into(),
                        snapshot.observed_at.clone().into(),
                        nozzle_temperatures_json.into(),
                        snapshot.active_nozzle.clone().into(),
                        snapshot.bed_temperature_celsius.clone().into(),
                        snapshot.bed_target_temperature_celsius.clone().into(),
                        snapshot.chamber_temperature_celsius.clone().into(),
                        snapshot.chamber_target_temperature_celsius.clone().into(),
                        snapshot.chamber_light_on.into(),
                        bambu_fun_bits.into(),
                        bambu_fun_session_id.into(),
                        bambu_nozzle_system_json.into(),
                        bambu_nozzle_system_session_id.into(),
                        mqtt_presence_session_id.into(),
                        snapshot.connection_authoritative.into(),
                        telemetry_write.status.into(),
                        telemetry_write.nozzle_temperatures.into(),
                        telemetry_write.active_nozzle.into(),
                        telemetry_write.bed_temperature.into(),
                        telemetry_write.bed_target_temperature.into(),
                        telemetry_write.chamber_temperature.into(),
                        telemetry_write.chamber_target_temperature.into(),
                        telemetry_write.chamber_light.into(),
                    ],
                ))
                .await
                .context("failed to upsert PostgreSQL printer snapshot")?;
        }
        backend => unreachable!("unsupported printer snapshot backend {backend:?}"),
    }

    Ok(())
}
