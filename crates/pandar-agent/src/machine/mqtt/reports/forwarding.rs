#[cfg(test)]
mod tests;

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
        MaterialRefreshResult, materials::normalize_material_patch,
    },
    protocol::agent::v1::AgentEvent,
};

use super::{
    print_job_report_event, print_report_from_parsed_report, printer_materials_snapshot_event,
    printer_snapshot_event,
};
use crate::machine::mqtt::{
    BAMBU_MQTT_QOS, BambuMqttCommand, BambuMqttTopics, BambuMqttTransport, MachineReports,
    PublishedMqttCommand, feature_event, is_mqtt_report_idle_timeout, snapshot_from_parsed_report,
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

fn snapshot_has_telemetry(snapshot: &MachineSnapshot) -> bool {
    snapshot.telemetry_authoritative
        || snapshot.state.is_some()
        || !snapshot.nozzle_temperatures.is_empty()
        || snapshot.active_nozzle.is_some()
        || snapshot.bed_temperature_celsius.is_some()
        || snapshot.bed_target_temperature_celsius.is_some()
        || snapshot.chamber_temperature_celsius.is_some()
        || snapshot.chamber_target_temperature_celsius.is_some()
        || snapshot.chamber_light_on.is_some()
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
    let (pushall, sequence_id) = pushall_command(&topics.request);
    reports
        .publish(pushall)
        .await
        .with_context(|| format!("publish pushall to request topic {}", topics.request))?;
    let mut outstanding_pushall = Some(sequence_id);

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
                let (pushall, sequence_id) = pushall_command(&topics.request);
                reports
                    .publish(pushall)
                    .await
                    .with_context(|| {
                        format!(
                            "publish periodic pushall to request topic {}",
                            topics.request
                        )
                    })?;
                outstanding_pushall = Some(sequence_id);
            },
            report = reports.next_report(report_timeout) => {
                match report {
            Ok(report) => {
                if let Some(processor) = &mut firmware_processor {
                    processor.observe(config, &report, sender).await?;
                }
                let observed_at = created_at_now();
                let printer_materials_json = report
                    .materials()
                    .and_then(|report| normalize_material_patch(report, &observed_at))
                    .and_then(|patch| serde_json::to_string(&patch).ok())
                    .unwrap_or_default();
                let progress = print_report_from_parsed_report(
                    endpoint,
                    report.print(),
                    report.raw_print_payload(),
                    observed_at,
                    printer_materials_json,
                );
                let device_features = match report.device_feature_observation(&endpoint.serial) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::warn!(
                            serial = %endpoint.serial,
                            error = %format!("{error:#}"),
                            "invalid printer device feature observation"
                        );
                        None
                    }
                };
                if let Some(value) = device_features {
                    context.device_features.update(&endpoint.serial, value).await;
                }
                let mut snapshot = snapshot_from_parsed_report(endpoint, report.snapshot());
                snapshot.telemetry_authoritative = outstanding_pushall.as_deref().is_some_and(
                    |sequence_id| {
                        report
                            .snapshot()
                            .is_some_and(|report| report.is_full_push_status(sequence_id))
                    },
                );
                if snapshot.telemetry_authoritative {
                    outstanding_pushall = None;
                }
                snapshot.model = None;
                snapshot.device_features = device_features;
                if context.presence.offline && !snapshot.telemetry_authoritative {
                    snapshot.state = None;
                }
                let restores_presence = snapshot.telemetry_authoritative;
                let snapshot_event = snapshot_has_telemetry(&snapshot)
                    .then(|| printer_snapshot_event(config, snapshot));
                let feature_event = (snapshot_event.is_none() && device_features.is_some())
                    .then(|| feature_event(config, endpoint.serial.clone(), device_features));
                let materials =
                    (!progress.printer_materials_json.is_empty()).then(|| MaterialRefreshResult {
                        serial: progress.serial.clone(),
                        printer_id: None,
                        printer_materials_json: progress.printer_materials_json.clone(),
                    });
                if report.has_non_firmware_print_telemetry()
                    && !report.is_feature_only_report()
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

fn pushall_command(topic: &str) -> (PublishedMqttCommand, String) {
    let command = BambuMqttCommand::RequestPushAll.command_payload();
    let sequence_id = command
        .sequence_id
        .expect("pushall commands always carry a sequence id");
    (
        PublishedMqttCommand {
            topic: topic.to_owned(),
            payload: command.payload,
            qos: BAMBU_MQTT_QOS,
        },
        sequence_id,
    )
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
        device_features: None,
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
