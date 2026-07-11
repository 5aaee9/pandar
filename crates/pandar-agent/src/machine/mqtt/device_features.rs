use std::time::Duration;

use anyhow::{Context, anyhow};
use pandar_core::BambuDeviceFeatures;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    AgentConfig,
    machine::{BambuPrinterEndpoint, DeviceFeatureCache},
    protocol::agent::v1::{
        AgentEvent, PrinterDeviceFeatures, PrinterDeviceFeaturesSnapshot, agent_event,
    },
};

use super::{
    BAMBU_MQTT_QOS, BambuMqttCommand, BambuMqttTopics, BambuMqttTransport, PublishedMqttCommand,
    SnapshotReport, parse_snapshot_report,
};

#[derive(Debug, Default)]
pub(super) enum FunField {
    #[default]
    Missing,
    String(String),
    Invalid,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PresentFun {
    String(String),
    Invalid(serde::de::IgnoredAny),
}

pub(super) fn deserialize_fun_field<'de, D>(deserializer: D) -> Result<FunField, D::Error>
where
    D: serde::Deserializer<'de>,
{
    PresentFun::deserialize(deserializer).map(|value| match value {
        PresentFun::String(value) => FunField::String(value),
        PresentFun::Invalid(_) => FunField::Invalid,
    })
}

pub(crate) fn device_feature_observation(
    serial: &str,
    report: &SnapshotReport,
) -> anyhow::Result<Option<BambuDeviceFeatures>> {
    match &report.print.as_ref().map(|print| &print.fun) {
        None | Some(FunField::Missing) => Ok(None),
        Some(FunField::String(value)) => BambuDeviceFeatures::from_hex(value)
            .with_context(|| format!("parse printer {serial} print.fun"))
            .map(Some),
        Some(FunField::Invalid) => Err(anyhow!(
            "printer {serial} print.fun expected a hexadecimal string"
        )),
    }
}

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
                    bambu_fun_bits: features.bits(),
                }),
            },
        )),
    }
}

pub(super) fn is_feature_only_report(report: &Value) -> bool {
    report.as_object().is_some_and(|fields| fields.len() == 1)
        && report
            .get("print")
            .and_then(Value::as_object)
            .is_some_and(|fields| fields.len() == 1 && fields.contains_key("fun"))
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
    let topics = BambuMqttTopics::for_serial(&endpoint.serial);
    transport
        .subscribe(&topics.report)
        .await
        .with_context(|| format!("subscribe to report topic {}", topics.report))?;
    transport
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
        let report = tokio::time::timeout(remaining, transport.next_report(remaining))
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
        let Some(report) = parse_snapshot_report(&report) else {
            continue;
        };
        let Some(value) = device_feature_observation(&endpoint.serial, &report)? else {
            continue;
        };
        cache.update(&endpoint.serial, value).await;
        return Ok(value);
    }
}
