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
        MaterialRefreshResult,
        materials::{normalize_material_patch, parse_materials_report},
    },
    protocol::agent::v1::AgentEvent,
};

use super::{
    parse_print_report, print_job_report_event, print_report_from_parsed_report,
    printer_materials_snapshot_event, printer_snapshot_event,
};
use crate::machine::mqtt::device_features::is_feature_only_report;
use crate::machine::mqtt::{
    BAMBU_MQTT_QOS, BambuMqttCommand, BambuMqttTopics, BambuMqttTransport, PublishedMqttCommand,
    device_feature_observation, feature_event, is_mqtt_report_idle_timeout, parse_snapshot_report,
    snapshot_from_parsed_report,
};
const PRINTER_REFRESH_INTERVAL: Duration = Duration::from_secs(60);

fn snapshot_has_temperature_telemetry(snapshot: &MachineSnapshot) -> bool {
    !snapshot.nozzle_temperatures.is_empty()
        || snapshot.bed_temperature_celsius.is_some()
        || snapshot.bed_target_temperature_celsius.is_some()
        || snapshot.chamber_temperature_celsius.is_some()
        || snapshot.chamber_target_temperature_celsius.is_some()
        || snapshot.chamber_light_on.is_some()
}

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
    forward_print_reports_inner(
        config,
        transport,
        endpoint,
        report_timeout,
        sender,
        cache,
        None,
    )
    .await
}

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
    forward_print_reports_inner(
        config,
        transport,
        endpoint,
        report_timeout,
        sender,
        cache,
        Some(firmware),
    )
    .await
}

async fn forward_print_reports_inner<T>(
    config: &AgentConfig,
    transport: &T,
    endpoint: &BambuPrinterEndpoint,
    report_timeout: Duration,
    sender: &mpsc::Sender<AgentEvent>,
    cache: &DeviceFeatureCache,
    firmware_context: Option<FirmwareReportContext>,
) -> anyhow::Result<()>
where
    T: BambuMqttTransport + ?Sized,
{
    let topics = BambuMqttTopics::for_serial(&endpoint.serial);
    transport
        .subscribe(&topics.report)
        .await
        .with_context(|| format!("subscribe to report topic {}", topics.report))?;
    let mut firmware_processor = if let Some(context) = firmware_context {
        Some(
            super::firmware::FirmwareReportProcessor::start(
                endpoint,
                context,
                report_timeout,
                transport,
                &topics,
            )
            .await?,
        )
    } else {
        None
    };
    transport
        .publish(PublishedMqttCommand {
            topic: topics.request.clone(),
            payload: BambuMqttCommand::RequestPushAll.payload(),
            qos: BAMBU_MQTT_QOS,
        })
        .await
        .with_context(|| format!("publish pushall to request topic {}", topics.request))?;

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
                transport
                    .publish(PublishedMqttCommand {
                        topic: topics.request.clone(),
                        payload: BambuMqttCommand::RequestPushAll.payload(),
                        qos: BAMBU_MQTT_QOS,
                    })
                    .await
                    .with_context(|| {
                        format!(
                            "publish periodic pushall to request topic {}",
                            topics.request
                        )
                    })?;
            },
            report = transport.next_report(report_timeout) => {
                match report {
            Ok(report) => {
                if let Some(processor) = &mut firmware_processor {
                    processor.observe(config, &report, sender).await?;
                }
                let observed_at = created_at_now();
                let print_report = parse_print_report(&report);
                let materials_report = parse_materials_report(&report);
                let printer_materials_json = materials_report
                    .as_ref()
                    .and_then(|report| normalize_material_patch(report, &observed_at))
                    .and_then(|patch| serde_json::to_string(&patch).ok())
                    .unwrap_or_default();
                let progress = print_report_from_parsed_report(
                    endpoint,
                    print_report.as_ref(),
                    super::raw_print_payload(&report),
                    observed_at,
                    printer_materials_json,
                );
                let snapshot_report = parse_snapshot_report(&report);
                let device_features = match snapshot_report
                    .as_ref()
                    .map(|report| device_feature_observation(&endpoint.serial, report))
                    .transpose()
                {
                    Ok(value) => value.flatten(),
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
                    cache.update(&endpoint.serial, value).await;
                }
                let mut snapshot = snapshot_from_parsed_report(endpoint, snapshot_report.as_ref());
                snapshot.device_features = device_features;
                let snapshot_event = snapshot_has_temperature_telemetry(&snapshot)
                    .then(|| printer_snapshot_event(config, snapshot));
                let feature_event = (snapshot_event.is_none() && device_features.is_some())
                    .then(|| feature_event(config, endpoint.serial.clone(), device_features));
                let materials =
                    (!progress.printer_materials_json.is_empty()).then(|| MaterialRefreshResult {
                        serial: progress.serial.clone(),
                        printer_id: None,
                        printer_materials_json: progress.printer_materials_json.clone(),
                    });
                if crate::machine::mqtt::has_non_firmware_print_telemetry(&report)
                    && !is_feature_only_report(&report)
                    && sender
                        .send(print_job_report_event(config, progress))
                        .await
                        .is_err()
                {
                    break;
                }
                if let Some(snapshot_event) = snapshot_event
                    && sender.send(snapshot_event).await.is_err()
                {
                    break;
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
