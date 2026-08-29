use std::time::Duration;

use anyhow::{Context, anyhow};
use pandar_core::{BambuDeviceFeatures, created_at_now};

use crate::{
    AgentConfig,
    machine::{BambuPrinterEndpoint, DeviceFeatureCache},
};
use pandar_protocol::agent::v1::{
    AgentEvent, PrinterDeviceFeatures, PrinterDeviceFeaturesSnapshot, agent_event,
};

use super::{
    BAMBU_MQTT_QOS, BambuMqttCommand, BambuMqttTopics, BambuMqttTransport, MachineReports,
    PublishedMqttCommand,
};

pub(crate) fn feature_event(
    config: &AgentConfig,
    serial: String,
    value: Option<BambuDeviceFeatures>,
) -> AgentEvent {
    AgentEvent {
        agent_id: config.agent_id.clone(),
        tenant_id: config.tenant_id.clone(),
        event_id: format!("printer-device-features-{serial}"),
        event: Some(agent_event::Event::PrinterDeviceFeaturesSnapshot(
            PrinterDeviceFeaturesSnapshot {
                serial,
                device_features: value.map(|features| PrinterDeviceFeatures {
                    bambu_fun_bits: Some(features.bits()),
                    bambu_fun2_bits: None,
                }),
            },
        )),
    }
}

pub(crate) async fn probe_device_features<T>(
    transport: &T,
    endpoint: &BambuPrinterEndpoint,
    report_timeout: Duration,
    cache: &DeviceFeatureCache,
) -> anyhow::Result<BambuDeviceFeatures>
where
    T: BambuMqttTransport + ?Sized,
{
    let value = observe_device_features(transport, endpoint, report_timeout).await?;
    cache.update(&endpoint.serial, value).await;
    Ok(value)
}

pub(crate) async fn observe_device_features<T>(
    transport: &T,
    endpoint: &BambuPrinterEndpoint,
    report_timeout: Duration,
) -> anyhow::Result<BambuDeviceFeatures>
where
    T: BambuMqttTransport + ?Sized,
{
    let reports = MachineReports::new(transport);
    let topics = BambuMqttTopics::for_serial(&endpoint.serial);
    reports
        .subscribe(&topics.report)
        .await
        .with_context(|| format!("subscribe to report topic {}", topics.report))?;
    reports
        .publish(PublishedMqttCommand {
            topic: topics.request.clone(),
            payload: BambuMqttCommand::RequestPushAll.payload(),
            qos: BAMBU_MQTT_QOS,
        })
        .await
        .with_context(|| format!("publish pushall to request topic {}", topics.request))?;

    let deadline = tokio::time::Instant::now() + report_timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(anyhow!(
                "timed out probing printer {} device features on report topic {}",
                endpoint.serial,
                topics.report
            ));
        }
        let report = tokio::time::timeout(remaining, reports.next_report(remaining))
            .await
            .map_err(|_| {
                anyhow!(
                    "timed out probing printer {} device features on report topic {}",
                    endpoint.serial,
                    topics.report
                )
            })?
            .with_context(|| {
                format!(
                    "probe printer {} device features on report topic {}",
                    endpoint.serial, topics.report
                )
            })?;
        let interpreted = report.interpret(endpoint, created_at_now());
        if let Some(diagnostic) = interpreted
            .diagnostics
            .into_iter()
            .find(|diagnostic| diagnostic.is_primary_device_features())
        {
            return Err(diagnostic.source)
                .context("interpret printer primary device feature observation");
        }
        let Some(value) = interpreted.features.primary else {
            continue;
        };
        return Ok(value);
    }
}
