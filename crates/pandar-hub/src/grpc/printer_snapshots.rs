use pandar_core::{
    AgentId, BambuNozzleDevice, BambuNozzleHolder, BambuNozzleInfo, BambuNozzleSystem,
    StudioFiniteF64, TenantId, valid_physical_nozzle_id,
};
use tonic::Status;

use crate::{
    AppState,
    printer_events::{PrinterEvent, fence_printer_nozzle_system, printer_event_printer},
    protocol::agent::v1::PrinterSnapshot,
    repositories::{PrinterSnapshotUpsert, RepositoryError},
    sessions::SessionToken,
};

pub async fn handle_snapshot(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    snapshot: PrinterSnapshot,
) -> Result<(), Status> {
    let connection_authoritative = snapshot.connection_authoritative;
    let serial_number = required(&snapshot.serial, "serial must not be blank")?;
    let name = required(&snapshot.name, "name must not be blank")?;
    let status = trim_optional(snapshot.state);
    let model = trim_optional(snapshot.model);
    let (device_features, device_features2) =
        snapshot.device_features.map_or((None, None), |features| {
            (
                features
                    .bambu_fun_bits
                    .map(pandar_core::BambuDeviceFeatures::from_bits),
                features
                    .bambu_fun2_bits
                    .map(pandar_core::BambuDeviceFeatures::from_bits),
            )
        });
    let nozzle_system = snapshot
        .nozzle_system
        .map(proto_nozzle_system)
        .transpose()?;
    let observed_at = pandar_core::created_at_now();

    let snapshot = PrinterSnapshotUpsert {
        serial_number,
        host: trim_optional(snapshot.host),
        access_code: trim_optional(snapshot.access_code),
        name,
        model,
        status,
        observed_at,
        nozzle_temperatures: snapshot
            .nozzle_temperatures
            .into_iter()
            .map(|temperature| pandar_core::PrinterNozzleTemperature {
                label: trim_optional(temperature.label),
                current_celsius: trim_optional(temperature.current_celsius),
                target_celsius: trim_optional(temperature.target_celsius),
                diameter_mm: trim_optional(temperature.diameter_mm),
                nozzle_type: trim_optional(temperature.nozzle_type),
                snow: temperature.snow,
                hnow: temperature.hnow,
            })
            .collect(),
        active_nozzle: trim_optional(snapshot.active_nozzle),
        bed_temperature_celsius: trim_optional(snapshot.bed_temperature_celsius),
        bed_target_temperature_celsius: trim_optional(snapshot.bed_target_temperature_celsius),
        chamber_temperature_celsius: trim_optional(snapshot.chamber_temperature_celsius),
        chamber_target_temperature_celsius: trim_optional(
            snapshot.chamber_target_temperature_celsius,
        ),
        chamber_light_on: snapshot.chamber_light_on,
        nozzle_system,
        connection_authoritative: snapshot.connection_authoritative,
        telemetry_authoritative: snapshot.telemetry_authoritative,
    };
    let _lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    if !state.sessions().is_current(agent_id, token).await {
        return Ok(());
    }
    let printer = match state
        .printers()
        .upsert_snapshot_with_device_features_if_current(
            tenant_id,
            agent_id,
            &token.persisted_id(),
            snapshot,
            device_features,
        )
        .await
    {
        Ok(printer) => printer,
        Err(RepositoryError::AgentSessionNotCurrent) => return Ok(()),
        Err(err) => return Err(repository_status(err)),
    };
    if let Some(features) = device_features2 {
        state
            .printers()
            .update_secondary_device_features_if_current(
                tenant_id,
                agent_id,
                &token.persisted_id(),
                &printer.serial_number,
                Some(features),
            )
            .await
            .map_err(repository_status)?;
    }
    if connection_authoritative {
        match state
            .materials()
            .clear_for_printer_if_current(&token.persisted_id(), tenant_id, agent_id, &printer.id)
            .await
        {
            Ok(()) => {}
            Err(RepositoryError::AgentSessionNotCurrent) => return Ok(()),
            Err(err) => return Err(repository_status(err)),
        }
    }
    let printer_id = printer.id;
    let printer = state
        .printers()
        .get_with_live_status_for_tenant(tenant_id, &printer_id)
        .await
        .map_err(repository_status)?
        .ok_or_else(|| Status::internal("printer snapshot missing after commit"))?;
    let materials = state
        .materials()
        .latest_for_printer(tenant_id, &printer_id)
        .await
        .map_err(repository_status)?;
    let printer = fence_printer_nozzle_system(state.sessions(), tenant_id, printer).await;
    state
        .publish_printer_event(
            tenant_id,
            PrinterEvent::PrinterSnapshot {
                printer: Box::new(printer_event_printer(printer, materials)),
            },
        )
        .await;

    Ok(())
}

fn proto_nozzle_system(
    system: crate::protocol::agent::v1::PrinterNozzleSystem,
) -> Result<BambuNozzleSystem, Status> {
    let nozzle = system
        .nozzle
        .ok_or_else(|| Status::invalid_argument("nozzle system requires nozzle data"))?;
    if nozzle.info.is_empty() {
        return Err(Status::invalid_argument(
            "nozzle system requires at least one nozzle",
        ));
    }
    let mut info = Vec::with_capacity(nozzle.info.len());
    for value in nozzle.info {
        if !valid_physical_nozzle_id(value.id)
            || info
                .iter()
                .any(|existing: &BambuNozzleInfo| existing.id == value.id)
            || !value.diameter.is_finite()
            || !(0.0..=0.8).contains(&value.diameter)
            || value.nozzle_type.trim().is_empty()
            || value.nozzle_type.len() > 32
            || value
                .wear
                .is_some_and(|wear| !wear.is_finite() || wear < 0.0)
        {
            return Err(Status::invalid_argument("invalid nozzle system entry"));
        }
        info.push(BambuNozzleInfo {
            id: value.id,
            diameter: StudioFiniteF64::try_from(f64::from(value.diameter))
                .map_err(|_| Status::invalid_argument("invalid nozzle diameter"))?,
            nozzle_type: value.nozzle_type,
            stat: value.stat,
            fila_id: value.fila_id,
            wear: value
                .wear
                .map(|wear| StudioFiniteF64::try_from(f64::from(wear)))
                .transpose()
                .map_err(|_| Status::invalid_argument("invalid nozzle wear"))?,
            p_t: value.print_time,
            color_m: value.color,
        });
    }
    info.sort_by_key(|value| value.id);
    Ok(BambuNozzleSystem {
        nozzle: BambuNozzleDevice {
            exist: nozzle.exist,
            state: nozzle.state,
            src_id: nozzle
                .src_id
                .filter(|value| valid_physical_nozzle_id(*value)),
            tar_id: nozzle
                .tar_id
                .filter(|value| valid_physical_nozzle_id(*value)),
            info,
        },
        holder: system.holder.map(|holder| BambuNozzleHolder {
            stat: holder.stat.filter(|value| (-1..=9).contains(value)),
            pos: holder.pos.filter(|value| (0..=3).contains(value)),
            info: holder.info.filter(|value| (-1..=1).contains(value)),
        }),
    })
}

fn required(value: &str, message: &'static str) -> Result<String, Status> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Status::invalid_argument(message));
    }

    Ok(value.to_string())
}

fn trim_optional(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn repository_status(err: RepositoryError) -> Status {
    match err {
        RepositoryError::MissingAgent => Status::not_found(err.to_string()),
        err => {
            tracing::error!(error = %format!("{err:#}"), "unexpected printer snapshot error");
            Status::internal("unexpected printer snapshot error")
        }
    }
}
