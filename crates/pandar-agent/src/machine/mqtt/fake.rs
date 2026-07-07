#![cfg(test)]

use std::{collections::VecDeque, sync::Arc, time::Duration};

use anyhow::bail;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::machine::materials::normalize_material_patch;

use super::{BambuMqttTransport, PublishedMqttCommand};

#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub struct FakeMqttTransport {
    state: Arc<Mutex<FakeMqttTransportState>>,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct FakeMqttTransportState {
    subscriptions: Vec<String>,
    published_commands: Vec<PublishedMqttCommand>,
    reports: VecDeque<Value>,
    timeout: bool,
    fail_publish_payload: Option<Value>,
    infinite_unrelated_reports: bool,
    last_material_report: Option<Value>,
    echo_operation_reports: bool,
    failed_led_node: Option<String>,
}

#[cfg(test)]
impl FakeMqttTransport {
    pub fn with_reports(reports: impl IntoIterator<Item = Value>) -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeMqttTransportState {
                reports: reports.into_iter().collect(),
                ..Default::default()
            })),
        }
    }

    pub fn with_timeout() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeMqttTransportState {
                timeout: true,
                ..Default::default()
            })),
        }
    }

    pub fn with_publish_failure(payload: Value) -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeMqttTransportState {
                fail_publish_payload: Some(payload),
                ..Default::default()
            })),
        }
    }

    pub fn with_infinite_unrelated_reports() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeMqttTransportState {
                infinite_unrelated_reports: true,
                ..Default::default()
            })),
        }
    }

    pub fn with_operation_reports() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeMqttTransportState {
                echo_operation_reports: true,
                ..Default::default()
            })),
        }
    }

    pub fn with_reports_and_operation_reports_failed_led_node(
        reports: impl IntoIterator<Item = Value>,
        led_node: &str,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeMqttTransportState {
                reports: reports.into_iter().collect(),
                echo_operation_reports: true,
                failed_led_node: Some(led_node.to_owned()),
                ..Default::default()
            })),
        }
    }

    pub async fn subscriptions(&self) -> Vec<String> {
        self.state.lock().await.subscriptions.clone()
    }

    pub async fn published_commands(&self) -> Vec<PublishedMqttCommand> {
        self.state.lock().await.published_commands.clone()
    }
}

#[cfg(test)]
#[async_trait]
impl BambuMqttTransport for FakeMqttTransport {
    async fn subscribe(&self, topic: &str) -> anyhow::Result<()> {
        self.state
            .lock()
            .await
            .subscriptions
            .push(topic.to_string());
        Ok(())
    }

    async fn publish(&self, command: PublishedMqttCommand) -> anyhow::Result<()> {
        let mut state = self.state.lock().await;
        if state
            .fail_publish_payload
            .as_ref()
            .is_some_and(|payload| published_payload_matches(payload, &command.payload))
        {
            bail!("fake publish failure");
        }
        if is_pushall_payload(&command.payload)
            && state.reports.is_empty()
            && let Some(report) = state.last_material_report.clone()
        {
            state.reports.push_back(report);
        }
        if state.echo_operation_reports
            && let Some(report) =
                operation_report_for_payload(&command.payload, state.failed_led_node.as_deref())
        {
            state.reports.push_back(report);
        }
        state.published_commands.push(command);
        Ok(())
    }

    async fn next_report(&self, _timeout: Duration) -> anyhow::Result<Value> {
        {
            let mut state = self.state.lock().await;
            if state.timeout {
                bail!("timed out waiting for MQTT report");
            }
            if let Some(report) = state.reports.pop_front() {
                if normalize_material_patch(&report, "2026-07-02T00:00:00Z").is_some() {
                    state.last_material_report = Some(report.clone());
                }
                return Ok(report);
            }
            if !state.infinite_unrelated_reports {
                bail!("timed out waiting for MQTT report");
            }
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
        Ok(json_value(FakeRunningReport {
            print: FakeRunningPrintReport {
                gcode_state: "RUNNING",
            },
        }))
    }
}

#[cfg(test)]
fn is_pushall_payload(payload: &Value) -> bool {
    serde_json::from_value::<FakeMqttPayload>(payload.clone())
        .ok()
        .and_then(|payload| payload.pushing)
        .and_then(|section| section.command)
        .and_then(|command| command.as_str().map(str::to_owned))
        == Some("pushall".to_owned())
}

#[cfg(test)]
fn published_payload_matches(expected: &Value, actual: &Value) -> bool {
    expected == actual
        || command_sections(&serde_json::from_value::<FakeMqttPayload>(expected.clone()).ok())
            .into_iter()
            .zip(command_sections(
                &serde_json::from_value::<FakeMqttPayload>(actual.clone()).ok(),
            ))
            .any(|(expected, actual)| expected.is_some() && expected == actual)
}

#[cfg(test)]
fn operation_report_for_payload(payload: &Value, failed_led_node: Option<&str>) -> Option<Value> {
    let payload = serde_json::from_value::<FakeMqttPayload>(payload.clone()).ok()?;
    if let Some(print) = payload.print {
        let command = print.command?;
        let sequence_id = print.sequence_id?;
        return Some(json_value(FakeOperationReport {
            print: Some(FakeOperationReportSection {
                command,
                sequence_id,
                result: "success",
                reason: None,
            }),
            system: None,
        }));
    }
    let system = payload.system?;
    let command = system.command?;
    let sequence_id = system.sequence_id?;
    if let Some(led_node) = system.led_node.as_deref()
        && Some(led_node) == failed_led_node
    {
        return Some(json_value(FakeOperationReport {
            print: None,
            system: Some(FakeOperationReportSection {
                command,
                sequence_id,
                result: "fail",
                reason: Some(format!("did not find the valid led: {led_node}")),
            }),
        }));
    }
    Some(json_value(FakeOperationReport {
        print: None,
        system: Some(FakeOperationReportSection {
            command,
            sequence_id,
            result: "success",
            reason: None,
        }),
    }))
}

#[cfg(test)]
fn json_value(value: impl Serialize) -> Value {
    serde_json::to_value(value).expect("fake MQTT report is serializable")
}

#[cfg(test)]
#[derive(Debug, Serialize)]
struct FakeRunningReport {
    print: FakeRunningPrintReport,
}

#[cfg(test)]
#[derive(Debug, Serialize)]
struct FakeRunningPrintReport {
    gcode_state: &'static str,
}

#[cfg(test)]
#[derive(Debug, Serialize)]
struct FakeOperationReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    print: Option<FakeOperationReportSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<FakeOperationReportSection>,
}

#[cfg(test)]
#[derive(Debug, Serialize)]
struct FakeOperationReportSection {
    command: Value,
    sequence_id: Value,
    result: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
struct FakeMqttPayload {
    #[serde(default)]
    info: Option<FakeCommandSection>,
    #[serde(default)]
    pushing: Option<FakeCommandSection>,
    #[serde(default)]
    print: Option<FakeCommandSection>,
    #[serde(default)]
    system: Option<FakeCommandSection>,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
struct FakeCommandSection {
    #[serde(default)]
    command: Option<Value>,
    #[serde(default)]
    sequence_id: Option<Value>,
    #[serde(default)]
    led_node: Option<String>,
}

#[cfg(test)]
fn command_sections(payload: &Option<FakeMqttPayload>) -> [Option<&Value>; 4] {
    let Some(payload) = payload else {
        return [None, None, None, None];
    };
    [
        payload
            .info
            .as_ref()
            .and_then(|section| section.command.as_ref()),
        payload
            .pushing
            .as_ref()
            .and_then(|section| section.command.as_ref()),
        payload
            .print
            .as_ref()
            .and_then(|section| section.command.as_ref()),
        payload
            .system
            .as_ref()
            .and_then(|section| section.command.as_ref()),
    ]
}
