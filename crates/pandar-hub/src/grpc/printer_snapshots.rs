use pandar_core::{
    AgentId, BambuDeviceFeatures, BambuNozzleDevice, BambuNozzleHolder, BambuNozzleInfo,
    BambuNozzleSystem, PrinterCoolingFan, PrinterCoolingFanKind, PrinterCoolingMode,
    PrinterCoolingSystem, StudioFiniteF64, TenantId, valid_physical_nozzle_id,
};
use tonic::Status;

use crate::{
    AppState,
    printer_events::{PrinterEvent, fence_printer_nozzle_system, printer_event_printer},
    repositories::{PrinterSnapshotUpsert, RepositoryError},
    sessions::SessionToken,
};
use pandar_protocol::agent::v1::PrinterSnapshot;

pub(super) struct ParsedPrinterSnapshot {
    upsert: PrinterSnapshotUpsert,
    device_features: Option<BambuDeviceFeatures>,
    device_features2: Option<BambuDeviceFeatures>,
}

pub async fn handle_snapshot(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    snapshot: PrinterSnapshot,
) -> Result<(), Status> {
    let parsed = parse_snapshot(snapshot)?;
    apply_snapshot(state, tenant_id, agent_id, token, parsed).await
}

pub(super) fn parse_snapshot(snapshot: PrinterSnapshot) -> Result<ParsedPrinterSnapshot, Status> {
    let serial_number = required(&snapshot.serial, "serial must not be blank")?;
    let name = required(&snapshot.name, "name must not be blank")?;
    let status = trim_optional(snapshot.state);
    let model = trim_optional(snapshot.model);
    let (device_features, device_features2) = snapshot
        .device_features
        .map_or((None, None), pandar_protocol::core_device_features);
    let nozzle_system = snapshot
        .nozzle_system
        .map(proto_nozzle_system)
        .transpose()?;
    let cooling_system = snapshot
        .cooling_system
        .map(proto_cooling_system)
        .transpose()?;
    let observed_at = pandar_core::created_at_now();

    let upsert = PrinterSnapshotUpsert {
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
        cooling_system,
        nozzle_system,
        connection_authoritative: snapshot.connection_authoritative,
        telemetry_authoritative: snapshot.telemetry_authoritative,
    };
    Ok(ParsedPrinterSnapshot {
        upsert,
        device_features,
        device_features2,
    })
}

pub(super) async fn apply_snapshot(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    token: SessionToken,
    parsed: ParsedPrinterSnapshot,
) -> Result<(), Status> {
    let ParsedPrinterSnapshot {
        upsert,
        device_features,
        device_features2,
    } = parsed;
    #[cfg(test)]
    let fanout_serial = upsert.serial_number.clone();
    let _lease = state
        .sessions()
        .transition_lease_for_session(agent_id, token)
        .await;
    if !state.sessions().is_current(agent_id, token).await {
        return Ok(());
    }
    let printer = match state
        .printers()
        .apply_snapshot_if_current(
            tenant_id,
            agent_id,
            &token.persisted_id(),
            upsert,
            device_features,
            device_features2,
        )
        .await
    {
        Ok(printer) => printer,
        Err(RepositoryError::AgentSessionNotCurrent) => return Ok(()),
        Err(err) => return Err(repository_status(err)),
    };
    // The current-session aggregate is committed; release the transition lease
    // so concurrent snapshot applications on the same agent stream only
    // serialize on the database fence, not on the event fanout below.
    drop(_lease);
    #[cfg(test)]
    snapshot_fanout_pause::wait(&fanout_serial).await;
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
    let serial_number = printer.printer.serial_number.clone();
    state
        .publish_printer_event(
            tenant_id,
            PrinterEvent::PrinterSnapshot {
                printer: Box::new(printer_event_printer(printer, materials)),
            },
        )
        .await;
    state
        .publish_printer_projection_change(tenant_id, &printer_id, &serial_number)
        .await;

    Ok(())
}

fn proto_cooling_system(
    system: pandar_protocol::agent::v1::PrinterCoolingSystem,
) -> Result<PrinterCoolingSystem, Status> {
    use pandar_protocol::agent::v1::{
        PrinterCoolingFanKind as ProtoFanKind, PrinterCoolingMode as ProtoMode,
    };

    let mode = system
        .mode
        .map(|mode| {
            ProtoMode::try_from(mode)
                .map_err(|_| Status::invalid_argument("invalid cooling system mode"))
                .and_then(|mode| match mode {
                    ProtoMode::Cooling => Ok(PrinterCoolingMode::Cooling),
                    ProtoMode::Heating => Ok(PrinterCoolingMode::Heating),
                    ProtoMode::Exhaust => Ok(PrinterCoolingMode::Exhaust),
                    ProtoMode::FullCooling => Ok(PrinterCoolingMode::FullCooling),
                    ProtoMode::Unspecified => {
                        Err(Status::invalid_argument("invalid cooling system mode"))
                    }
                })
        })
        .transpose()?;
    let mut fans = Vec::with_capacity(system.fans.len());
    for fan in system.fans {
        let kind = match ProtoFanKind::try_from(fan.kind)
            .map_err(|_| Status::invalid_argument("invalid cooling fan kind"))?
        {
            ProtoFanKind::Hotend => PrinterCoolingFanKind::Hotend,
            ProtoFanKind::PartCooling => PrinterCoolingFanKind::PartCooling,
            ProtoFanKind::Auxiliary => PrinterCoolingFanKind::Auxiliary,
            ProtoFanKind::Chamber => PrinterCoolingFanKind::Chamber,
            ProtoFanKind::HotendSecond => PrinterCoolingFanKind::HotendSecond,
            ProtoFanKind::Controller => PrinterCoolingFanKind::Controller,
            ProtoFanKind::InnerLoop => PrinterCoolingFanKind::InnerLoop,
            ProtoFanKind::AuxiliarySecond => PrinterCoolingFanKind::AuxiliarySecond,
            ProtoFanKind::Unspecified => {
                return Err(Status::invalid_argument("invalid cooling fan kind"));
            }
        };
        if fan.speed_percent > 100
            || fans
                .iter()
                .any(|value: &PrinterCoolingFan| value.kind == kind)
        {
            return Err(Status::invalid_argument("invalid cooling fan entry"));
        }
        fans.push(PrinterCoolingFan {
            kind,
            speed_percent: fan.speed_percent as u8,
        });
    }
    fans.sort_by_key(|fan| fan.kind);
    if mode.is_none() && fans.is_empty() {
        return Err(Status::invalid_argument("cooling system is empty"));
    }
    Ok(PrinterCoolingSystem { mode, fans })
}

fn proto_nozzle_system(
    system: pandar_protocol::agent::v1::PrinterNozzleSystem,
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

/// Test hook pausing the post-commit snapshot fanout for one printer serial so
/// tests can prove a slow fanout no longer blocks later events on the stream.
#[cfg(test)]
pub(crate) mod snapshot_fanout_pause {
    use std::{
        collections::HashMap,
        sync::{Mutex, OnceLock},
        time::Duration,
    };

    use tokio::sync::oneshot;

    const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

    struct PausePoint {
        reached: oneshot::Sender<()>,
        resume: oneshot::Receiver<()>,
    }

    pub(crate) struct FanoutPause {
        reached: oneshot::Receiver<()>,
        resume: Option<oneshot::Sender<()>>,
    }

    pub(crate) fn install(serial: &str) -> FanoutPause {
        let (reached_sender, reached_receiver) = oneshot::channel();
        let (resume_sender, resume_receiver) = oneshot::channel();
        let previous = pauses()
            .lock()
            .expect("snapshot fanout pause mutex should not be poisoned")
            .insert(
                serial.to_owned(),
                PausePoint {
                    reached: reached_sender,
                    resume: resume_receiver,
                },
            );
        assert!(
            previous.is_none(),
            "snapshot fanout pause already installed"
        );
        FanoutPause {
            reached: reached_receiver,
            resume: Some(resume_sender),
        }
    }

    impl FanoutPause {
        pub(crate) async fn wait_until_reached(&mut self) {
            tokio::time::timeout(WAIT_TIMEOUT, &mut self.reached)
                .await
                .expect("timed out waiting for snapshot fanout pause")
                .expect("snapshot fanout pause was dropped before being reached");
        }

        pub(crate) fn resume(mut self) {
            let _ = self
                .resume
                .take()
                .expect("snapshot fanout resume sender must be present")
                .send(());
        }
    }

    pub(crate) async fn wait(serial: &str) {
        let pause = pauses()
            .lock()
            .expect("snapshot fanout pause mutex should not be poisoned")
            .remove(serial);
        if let Some(pause) = pause {
            let _ = pause.reached.send(());
            let _ = pause.resume.await;
        }
    }

    fn pauses() -> &'static Mutex<HashMap<String, PausePoint>> {
        static PAUSES: OnceLock<Mutex<HashMap<String, PausePoint>>> = OnceLock::new();
        PAUSES.get_or_init(|| Mutex::new(HashMap::new()))
    }
}
