mod nozzle_system;
#[cfg(test)]
mod tests;

use nozzle_system::NozzleSystemReducer;

use std::time::Duration;

use anyhow::Context;
use pandar_core::created_at_now;
use tokio::{
    sync::mpsc,
    time::{Instant, MissedTickBehavior, interval_at},
};

use crate::{
    AgentConfig,
    machine::{
        BambuPrinterEndpoint, DeviceFeatureCache, FirmwareReportContext, MachineSnapshot,
        MaterialRefreshResult,
    },
    protocol::agent::v1::AgentEvent,
};

use super::{print_job_report_event, printer_materials_snapshot_event, printer_snapshot_event};
use crate::machine::mqtt::{
    BAMBU_MQTT_QOS, BambuMqttCommand, BambuMqttTopics, BambuMqttTransport, MachineReports,
    PrintTelemetryClass, PublishedMqttCommand, SnapshotAuthority, SnapshotContent, feature_event,
    is_mqtt_report_idle_timeout,
};
const PRINTER_REFRESH_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
pub(crate) struct MqttPresenceState {
    offline: bool,
}

pub(crate) struct MqttForwardingContext<'a> {
    pub(crate) device_features: &'a DeviceFeatureCache,
    pub(crate) firmware: Option<FirmwareReportContext>,
    pub(crate) presence: &'a mut MqttPresenceState,
}

#[cfg(test)]
pub async fn forward_print_reports<T>(
    config: &AgentConfig,
    transport: &T,
    endpoint: &BambuPrinterEndpoint,
    report_timeout: Duration,
    sender: &mpsc::Sender<AgentEvent>,
    cache: &DeviceFeatureCache,
) -> anyhow::Result<()>
where
    T: BambuMqttTransport + ?Sized,
{
    let mut presence = MqttPresenceState::default();
    forward_print_reports_with_context(
        config,
        transport,
        endpoint,
        report_timeout,
        sender,
        MqttForwardingContext {
            device_features: cache,
            firmware: None,
            presence: &mut presence,
        },
    )
    .await
}

#[cfg(test)]
pub async fn forward_print_reports_with_firmware<T>(
    config: &AgentConfig,
    transport: &T,
    endpoint: &BambuPrinterEndpoint,
    report_timeout: Duration,
    sender: &mpsc::Sender<AgentEvent>,
    cache: &DeviceFeatureCache,
    firmware: FirmwareReportContext,
) -> anyhow::Result<()>
where
    T: BambuMqttTransport + ?Sized,
{
    let mut presence = MqttPresenceState::default();
    forward_print_reports_with_context(
        config,
        transport,
        endpoint,
        report_timeout,
        sender,
        MqttForwardingContext {
            device_features: cache,
            firmware: Some(firmware),
            presence: &mut presence,
        },
    )
    .await
}

pub(crate) async fn forward_print_reports_with_context<T>(
    config: &AgentConfig,
    transport: &T,
    endpoint: &BambuPrinterEndpoint,
    report_timeout: Duration,
    sender: &mpsc::Sender<AgentEvent>,
    mut context: MqttForwardingContext<'_>,
) -> anyhow::Result<()>
where
    T: BambuMqttTransport + ?Sized,
{
    let result = forward_print_reports_inner(
        config,
        transport,
        endpoint,
        report_timeout,
        sender,
        &mut context,
    )
    .await;
    if result.is_err() {
        mark_mqtt_offline(config, endpoint, sender, context.presence).await;
    }
    result
}

async fn forward_print_reports_inner<T>(
    config: &AgentConfig,
    transport: &T,
    endpoint: &BambuPrinterEndpoint,
    report_timeout: Duration,
    sender: &mpsc::Sender<AgentEvent>,
    context: &mut MqttForwardingContext<'_>,
) -> anyhow::Result<()>
where
    T: BambuMqttTransport + ?Sized,
{
    let topics = BambuMqttTopics::for_serial(&endpoint.serial);
    let reports = MachineReports::new(transport);
    reports
        .subscribe(&topics.report)
        .await
        .with_context(|| format!("subscribe to report topic {}", topics.report))?;
    let mut firmware_processor = if let Some(firmware) = context.firmware.take() {
        Some(
            super::firmware::FirmwareReportProcessor::start(
                endpoint,
                firmware,
                report_timeout,
                &reports,
                &topics,
            )
            .await?,
        )
    } else {
        None
    };
    let pushall = pushall_command(&topics.request);
    reports
        .publish(pushall)
        .await
        .with_context(|| format!("publish pushall to request topic {}", topics.request))?;
    let mut nozzle_system = NozzleSystemReducer::default();

    let mut refresh_interval = interval_at(
        Instant::now() + PRINTER_REFRESH_INTERVAL,
        PRINTER_REFRESH_INTERVAL,
    );
    refresh_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            biased;
            _ = sender.closed() => break,
            _ = refresh_interval.tick() => {
                let pushall = pushall_command(&topics.request);
                reports
                    .publish(pushall)
                    .await
                    .with_context(|| {
                        format!(
                            "publish periodic pushall to request topic {}",
                            topics.request
                        )
                    })?;
            },
            report = reports.next_report(report_timeout) => {
                match report {
            Ok(report) => {
                if let Some(processor) = &mut firmware_processor {
                    processor.observe(config, &report, sender).await?;
                }
                let mut interpreted = report.interpret(endpoint, created_at_now());
                for diagnostic in &interpreted.diagnostics {
                    tracing::warn!(
                        serial = %endpoint.serial,
                        section = ?diagnostic.section,
                        issue = ?diagnostic.issue,
                        error = %format!("{diagnostic:#}"),
                        "invalid printer report observation"
                    );
                }
                let device_features = interpreted.features.primary;
                if let Some(value) = device_features {
                    context.device_features.update(&endpoint.serial, value).await;
                }
                if let Some(patch) = interpreted.nozzle_patch.take()
                    && let Some(snapshot) = &mut interpreted.snapshot
                {
                    snapshot.nozzle_system = nozzle_system.update(patch);
                }
                if let Some(snapshot) = &mut interpreted.snapshot {
                    snapshot.model = None;
                    if context.presence.offline && !snapshot.telemetry_authoritative {
                        snapshot.state = None;
                    }
                }
                let restores_presence =
                    interpreted.facts.authority == SnapshotAuthority::FullPushStatus;
                let snapshot_event = interpreted.snapshot.take().and_then(|snapshot| {
                    (interpreted.facts.snapshot == SnapshotContent::Telemetry
                        || snapshot.nozzle_system.is_some())
                    .then(|| printer_snapshot_event(config, snapshot))
                });
                let feature_event = (snapshot_event.is_none() && device_features.is_some())
                    .then(|| feature_event(config, endpoint.serial.clone(), device_features));
                let materials = interpreted.materials.map(|patch| MaterialRefreshResult {
                    serial: endpoint.serial.clone(),
                    printer_id: None,
                    printer_materials_json: patch.into_json(),
                });
                if interpreted.facts.print == PrintTelemetryClass::Operational
                    && let Some(progress) = interpreted.print
                    && sender
                        .send(print_job_report_event(config, progress))
                        .await
                        .is_err()
                {
                    break;
                }
                if let Some(snapshot_event) = snapshot_event
                {
                    if sender.send(snapshot_event).await.is_err() {
                        break;
                    }
                    if restores_presence {
                        context.presence.offline = false;
                    }
                }
                if let Some(feature_event) = feature_event
                    && sender.send(feature_event).await.is_err()
                {
                    break;
                }
                if let Some(materials) = materials
                    && sender
                        .send(printer_materials_snapshot_event(config, materials))
                        .await
                        .is_err()
                {
                    break;
                }
            }
            Err(err) if is_mqtt_report_idle_timeout(&err) => {
                if let Some(processor) = &mut firmware_processor {
                    processor.expire_version_observation();
                }
                tracing::warn!(
                    serial = %endpoint.serial,
                    error = %format!("{err:#}"),
                    "printer report receive failed"
                );
                if !mark_mqtt_offline(config, endpoint, sender, context.presence).await {
                    break;
                }
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!(
                        "receive printer {} report from topic {}",
                        endpoint.serial, topics.report
                    )
                });
            }
                }
            }
        }
    }

    Ok(())
}

fn pushall_command(topic: &str) -> PublishedMqttCommand {
    let command = BambuMqttCommand::RequestPushAll.command_payload();
    PublishedMqttCommand {
        topic: topic.to_owned(),
        payload: command.payload,
        qos: BAMBU_MQTT_QOS,
    }
}

async fn mark_mqtt_offline(
    config: &AgentConfig,
    endpoint: &BambuPrinterEndpoint,
    sender: &mpsc::Sender<AgentEvent>,
    presence: &mut MqttPresenceState,
) -> bool {
    if presence.offline {
        return true;
    }
    let snapshot = MachineSnapshot {
        serial: endpoint.serial.clone(),
        host: None,
        access_code: None,
        name: endpoint
            .name
            .clone()
            .unwrap_or_else(|| endpoint.serial.clone()),
        model: None,
        state: Some("offline".to_owned()),
        nozzle_temperatures: Vec::new(),
        active_nozzle: None,
        bed_temperature_celsius: None,
        bed_target_temperature_celsius: None,
        chamber_temperature_celsius: None,
        chamber_target_temperature_celsius: None,
        chamber_light_on: None,
        cooling_system: None,
        device_features: None,
        device_features2: None,
        nozzle_system: None,
        telemetry_authoritative: false,
    };
    if sender
        .send(printer_snapshot_event(config, snapshot))
        .await
        .is_err()
    {
        return false;
    }
    presence.offline = true;
    true
}
