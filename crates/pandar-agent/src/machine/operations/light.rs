use anyhow::Context;
use serde_json::Value;

use crate::machine::mqtt::{
    BAMBU_MQTT_QOS, BambuMqttCommand, BambuMqttTopics, BambuMqttTransport, PublishedMqttCommand,
    chamber_light_payloads_for_nodes,
};

const BAMBU_STUDIO_CHAMBER_LIGHT_NODES: [&str; 2] = ["chamber_light", "chamber_light2"];

pub(super) async fn chamber_light_payloads<T>(
    mqtt: &T,
    topics: &BambuMqttTopics,
    requested_on: Option<bool>,
) -> anyhow::Result<Vec<Value>>
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
    Ok(chamber_light_payloads_for_nodes(
        BAMBU_STUDIO_CHAMBER_LIGHT_NODES,
        requested_on.unwrap_or(!report.on),
    ))
}

struct ChamberLightReport {
    on: bool,
}

async fn latest_chamber_light_report<T>(mqtt: &T) -> anyhow::Result<ChamberLightReport>
where
    T: BambuMqttTransport + Send + Sync,
{
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let report = mqtt
                .next_report(std::time::Duration::from_secs(5))
                .await
                .context("wait for chamber light status report")?;
            if let Some(light_report) = chamber_light_report(&report) {
                return Ok(light_report);
            }
        }
    })
    .await
    .context("wait for chamber light status report")?
}

fn chamber_light_report(report: &Value) -> Option<ChamberLightReport> {
    let lights = report.pointer("/print/lights_report")?.as_array()?;
    let mut on = None;
    for light in lights {
        let Some(node @ ("chamber_light" | "chamber_light2")) =
            light.get("node").and_then(Value::as_str)
        else {
            continue;
        };
        if node == "chamber_light" || on.is_none() {
            on = Some(light.get("mode").and_then(Value::as_str) == Some("on"));
        }
    }

    Some(ChamberLightReport { on: on? })
}
