use anyhow::Context;
use pandar_core::{AgentId, BambuDeviceFeatures, TenantId};
use sea_orm::sea_query::{Alias, Expr, ExprTrait, OnConflict, Query};
use sea_orm::{ConnectionTrait, DatabaseTransaction};

use crate::{
    entities::printers,
    repositories::{PrinterSnapshotUpsert, RepositoryResult, printers::SnapshotSessionState},
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
    cooling_system: bool,
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
            cooling_system: authoritative || snapshot.cooling_system.is_some(),
        }
    }
}

// SeaORM's generic update path is select-then-write here; keep one SeaQuery
// statement so both backends preserve atomic ON CONFLICT snapshot semantics.
pub(crate) async fn upsert_snapshot(
    transaction: &DatabaseTransaction,
    tenant_id: TenantId,
    agent_id: AgentId,
    snapshot: &PrinterSnapshotUpsert,
    access_code_encrypted: Option<&str>,
    session_state: SnapshotSessionState<'_>,
) -> RepositoryResult<()> {
    let bambu_fun_bits = session_state
        .device_features
        .map(BambuDeviceFeatures::to_hex);
    let cooling_system_json = snapshot
        .cooling_system
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .context("failed to serialize printer cooling system")?;
    let bambu_nozzle_system_json = snapshot
        .nozzle_system
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .context("failed to serialize Bambu nozzle system")?;
    let nozzle_temperatures_json = serde_json::to_string(&snapshot.nozzle_temperatures)
        .context("failed to serialize nozzle temperatures")?;
    let telemetry_write = TelemetryWriteMask::from_snapshot(snapshot);
    let status = snapshot
        .status
        .clone()
        .unwrap_or_else(|| "unknown".to_owned());

    let excluded = |column| Expr::col((Alias::new("excluded"), column));
    let stored = |column| Expr::col((Alias::new("printers"), column));
    let selected = |write, column| {
        if write {
            excluded(column)
        } else {
            stored(column)
        }
    };
    let connection_value = |column| {
        if snapshot.connection_authoritative {
            excluded(column)
        } else {
            stored(column).if_null(excluded(column))
        }
    };

    let mut query = Query::insert();
    query
        .into_table(printers::Entity)
        .columns([
            printers::Column::Id,
            printers::Column::TenantId,
            printers::Column::AgentId,
            printers::Column::SerialNumber,
            printers::Column::Host,
            printers::Column::AccessCode,
            printers::Column::AccessCodeEncrypted,
            printers::Column::Name,
            printers::Column::Model,
            printers::Column::Status,
            printers::Column::LastSeenAt,
            printers::Column::CreatedAt,
            printers::Column::NozzleTemperaturesJson,
            printers::Column::ActiveNozzle,
            printers::Column::BedTemperatureCelsius,
            printers::Column::BedTargetTemperatureCelsius,
            printers::Column::ChamberTemperatureCelsius,
            printers::Column::ChamberTargetTemperatureCelsius,
            printers::Column::ChamberLightOn,
            printers::Column::CoolingSystemJson,
            printers::Column::BambuFunBits,
            printers::Column::BambuFunSessionId,
            printers::Column::BambuNozzleSystemJson,
            printers::Column::BambuNozzleSystemSessionId,
            printers::Column::MqttPresenceSessionId,
            printers::Column::StateRevision,
        ])
        .values_panic([
            Expr::val(uuid::Uuid::new_v4().to_string()),
            Expr::val(tenant_id.to_string()),
            Expr::val(agent_id.to_string()),
            Expr::val(snapshot.serial_number.clone()),
            Expr::val(snapshot.host.clone()),
            Expr::val(Option::<String>::None),
            Expr::val(access_code_encrypted.map(str::to_owned)),
            Expr::val(snapshot.name.clone()),
            Expr::val(snapshot.model.clone()),
            Expr::val(status),
            Expr::val(snapshot.observed_at.clone()),
            Expr::val(snapshot.observed_at.clone()),
            Expr::val(nozzle_temperatures_json),
            Expr::val(snapshot.active_nozzle.clone()),
            Expr::val(snapshot.bed_temperature_celsius.clone()),
            Expr::val(snapshot.bed_target_temperature_celsius.clone()),
            Expr::val(snapshot.chamber_temperature_celsius.clone()),
            Expr::val(snapshot.chamber_target_temperature_celsius.clone()),
            Expr::val(snapshot.chamber_light_on),
            Expr::val(cooling_system_json),
            Expr::val(bambu_fun_bits),
            Expr::val(session_state.device_features_session_id.map(str::to_owned)),
            Expr::val(bambu_nozzle_system_json),
            Expr::val(session_state.nozzle_system_session_id.map(str::to_owned)),
            Expr::val(session_state.mqtt_presence_session_id.map(str::to_owned)),
            Expr::val(1_i64),
        ])
        .on_conflict(
            OnConflict::columns([printers::Column::TenantId, printers::Column::SerialNumber])
                .values([
                    (
                        printers::Column::AgentId,
                        excluded(printers::Column::AgentId),
                    ),
                    (
                        printers::Column::Host,
                        connection_value(printers::Column::Host),
                    ),
                    (
                        printers::Column::AccessCode,
                        Expr::val(Option::<String>::None),
                    ),
                    (
                        printers::Column::AccessCodeEncrypted,
                        connection_value(printers::Column::AccessCodeEncrypted),
                    ),
                    (
                        printers::Column::Model,
                        excluded(printers::Column::Model).if_null(stored(printers::Column::Model)),
                    ),
                    (
                        printers::Column::Status,
                        selected(telemetry_write.status, printers::Column::Status),
                    ),
                    (
                        printers::Column::LastSeenAt,
                        excluded(printers::Column::LastSeenAt),
                    ),
                    (
                        printers::Column::NozzleTemperaturesJson,
                        selected(
                            telemetry_write.nozzle_temperatures,
                            printers::Column::NozzleTemperaturesJson,
                        ),
                    ),
                    (
                        printers::Column::ActiveNozzle,
                        selected(
                            telemetry_write.active_nozzle,
                            printers::Column::ActiveNozzle,
                        ),
                    ),
                    (
                        printers::Column::BedTemperatureCelsius,
                        selected(
                            telemetry_write.bed_temperature,
                            printers::Column::BedTemperatureCelsius,
                        ),
                    ),
                    (
                        printers::Column::BedTargetTemperatureCelsius,
                        selected(
                            telemetry_write.bed_target_temperature,
                            printers::Column::BedTargetTemperatureCelsius,
                        ),
                    ),
                    (
                        printers::Column::ChamberTemperatureCelsius,
                        selected(
                            telemetry_write.chamber_temperature,
                            printers::Column::ChamberTemperatureCelsius,
                        ),
                    ),
                    (
                        printers::Column::ChamberTargetTemperatureCelsius,
                        selected(
                            telemetry_write.chamber_target_temperature,
                            printers::Column::ChamberTargetTemperatureCelsius,
                        ),
                    ),
                    (
                        printers::Column::ChamberLightOn,
                        selected(
                            telemetry_write.chamber_light,
                            printers::Column::ChamberLightOn,
                        ),
                    ),
                    (
                        printers::Column::CoolingSystemJson,
                        selected(
                            telemetry_write.cooling_system,
                            printers::Column::CoolingSystemJson,
                        ),
                    ),
                    (
                        printers::Column::BambuFunBits,
                        excluded(printers::Column::BambuFunBits)
                            .if_null(stored(printers::Column::BambuFunBits)),
                    ),
                    (
                        printers::Column::BambuFunSessionId,
                        Expr::case(
                            excluded(printers::Column::BambuFunBits).is_null(),
                            stored(printers::Column::BambuFunSessionId),
                        )
                        .finally(excluded(printers::Column::BambuFunSessionId))
                        .into(),
                    ),
                    (
                        printers::Column::BambuNozzleSystemJson,
                        excluded(printers::Column::BambuNozzleSystemJson)
                            .if_null(stored(printers::Column::BambuNozzleSystemJson)),
                    ),
                    (
                        printers::Column::BambuNozzleSystemSessionId,
                        Expr::case(
                            excluded(printers::Column::BambuNozzleSystemJson).is_null(),
                            stored(printers::Column::BambuNozzleSystemSessionId),
                        )
                        .finally(excluded(printers::Column::BambuNozzleSystemSessionId))
                        .into(),
                    ),
                    (
                        printers::Column::MqttPresenceSessionId,
                        excluded(printers::Column::MqttPresenceSessionId)
                            .if_null(stored(printers::Column::MqttPresenceSessionId)),
                    ),
                    (
                        printers::Column::StateRevision,
                        stored(printers::Column::StateRevision).add(1),
                    ),
                ])
                .to_owned(),
        );

    transaction
        .execute(&query)
        .await
        .context("failed to upsert printer snapshot")?;
    Ok(())
}
