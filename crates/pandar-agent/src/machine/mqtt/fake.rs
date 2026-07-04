#![cfg(test)]

use std::{collections::VecDeque, sync::Arc, time::Duration};

use anyhow::bail;
use async_trait::async_trait;
use serde_json::{Value, json};
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
            && let Some(report) = operation_report_for_payload(&command.payload)
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
        Ok(json!({"print": {"gcode_state": "RUNNING"}}))
    }
}

#[cfg(test)]
fn is_pushall_payload(payload: &Value) -> bool {
    payload["pushing"]["command"].as_str() == Some("pushall")
}

#[cfg(test)]
fn published_payload_matches(expected: &Value, actual: &Value) -> bool {
    expected == actual
        || ["info", "pushing", "print", "system"]
            .into_iter()
            .any(|section| {
                expected[section]["command"].as_str().is_some()
                    && expected[section]["command"].as_str() == actual[section]["command"].as_str()
            })
}

#[cfg(test)]
fn operation_report_for_payload(payload: &Value) -> Option<Value> {
    if let Some(print) = payload.get("print") {
        return Some(json!({
            "print": {
                "command": print.get("command")?.clone(),
                "sequence_id": print.get("sequence_id")?.clone(),
                "result": "success"
            }
        }));
    }
    let system = payload.get("system")?;
    Some(json!({
        "system": {
            "command": system.get("command")?.clone(),
            "sequence_id": system.get("sequence_id")?.clone(),
            "result": "success"
        }
    }))
}
