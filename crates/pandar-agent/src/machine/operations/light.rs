use anyhow::Context;
use serde::Deserialize;
use serde_json::Value;

use crate::machine::mqtt::{
    BAMBU_MQTT_QOS, BambuMqttCommand, BambuMqttTopics, BambuMqttTransport, PublishedMqttCommand,
    chamber_light_commands_for_nodes,
};

const BAMBU_STUDIO_CHAMBER_LIGHT_NODES: [&str; 2] = ["chamber_light", "chamber_light2"];

pub(super) async fn chamber_light_commands<T>(
    mqtt: &T,
    topics: &BambuMqttTopics,
    requested_on: Option<bool>,
) -> anyhow::Result<Vec<BambuMqttCommand>>
where
    T: BambuMqttTransport + Send + Sync,
{
    mqtt.publish(PublishedMqttCommand {
        topic: topics.request.clone(),
        payload: BambuMqttCommand::RequestPushAll.payload(),
        qos: BAMBU_MQTT_QOS,
    })
    .await
    .context("request current light report before controlling chamber light")?;

    let report = latest_chamber_light_report(mqtt).await?;
    Ok(chamber_light_commands_for_nodes(
        BAMBU_STUDIO_CHAMBER_LIGHT_NODES,
        requested_on.unwrap_or(!report.on),
    ))
}

struct ChamberLightReport {
    on: bool,
}

#[derive(Debug, Deserialize)]
struct PrinterReport {
    print: Option<PrintSection>,
}

#[derive(Debug, Deserialize)]
struct PrintSection {
    lights_report: Option<Vec<LightReport>>,
}

#[derive(Debug, Deserialize)]
struct LightReport {
    node: Option<String>,
    mode: Option<String>,
}

async fn latest_chamber_light_report<T>(mqtt: &T) -> anyhow::Result<ChamberLightReport>
where
    T: BambuMqttTransport + Send + Sync,
{
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let Some(report) = next_printer_report(mqtt).await? else {
                continue;
            };
            if let Some(light_report) = report.chamber_light_report() {
                return Ok(light_report);
            }
        }
    })
    .await
    .context("wait for chamber light status report")?
}

async fn next_printer_report<T>(mqtt: &T) -> anyhow::Result<Option<PrinterReport>>
where
    T: BambuMqttTransport + Send + Sync,
{
    let report = mqtt
        .next_report(std::time::Duration::from_secs(5))
        .await
        .context("wait for chamber light status report")?;
    Ok(parse_printer_report(report))
}

fn parse_printer_report(report: Value) -> Option<PrinterReport> {
    serde_json::from_value(report).ok()
}

impl PrinterReport {
    fn chamber_light_report(self) -> Option<ChamberLightReport> {
        let lights = self.print?.lights_report?;
        let mut on = None;
        for light in lights {
            let Some(node @ ("chamber_light" | "chamber_light2")) = light.node.as_deref() else {
                continue;
            };
            if node == "chamber_light" || on.is_none() {
                on = Some(light.mode.as_deref() == Some("on"));
            }
        }

        Some(ChamberLightReport { on: on? })
    }
}
