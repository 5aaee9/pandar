use std::{collections::HashMap, sync::Arc};

use pandar_core::{CommandRecord, Printer, TenantId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Mutex, broadcast};

use crate::{
    metrics::{MetricsState, SubscriptionGuard},
    repositories::MaterialSnapshot,
    routes::jobs::JobResponse,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PrinterEvent {
    #[serde(rename = "printer_snapshot")]
    PrinterSnapshot { printer: Box<PrinterEventPrinter> },
    #[serde(rename = "job_progress")]
    JobProgress { job: Box<JobResponse> },
    #[serde(rename = "command_result")]
    CommandResult { command: Box<PrinterEventCommand> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrinterEventPrinter {
    pub id: String,
    pub tenant_id: String,
    pub agent_id: String,
    pub serial_number: String,
    pub name: String,
    pub model: Option<String>,
    pub status: String,
    pub last_seen_at: String,
    pub created_at: String,
    pub materials: Option<PrinterEventMaterials>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrinterEventMaterials {
    pub ams_units: Value,
    pub external_spools: Value,
    pub active_tray: Option<Value>,
    pub observed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrinterEventCommand {
    pub id: String,
    pub tenant_id: String,
    pub agent_id: String,
    pub printer_id: Option<String>,
    pub kind: String,
    pub status: String,
    pub payload_json: String,
    pub error: Option<String>,
    pub result_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub fn printer_event_printer(
    printer: Printer,
    materials: Option<MaterialSnapshot>,
) -> PrinterEventPrinter {
    PrinterEventPrinter {
        id: printer.id,
        tenant_id: printer.tenant_id.to_string(),
        agent_id: printer.agent_id.to_string(),
        serial_number: printer.serial_number,
        name: printer.name,
        model: printer.model,
        status: printer.status,
        last_seen_at: printer.last_seen_at,
        created_at: printer.created_at,
        materials: materials.map(PrinterEventMaterials::from),
    }
}

impl From<MaterialSnapshot> for PrinterEventMaterials {
    fn from(snapshot: MaterialSnapshot) -> Self {
        Self {
            ams_units: scrub_material_json(snapshot.ams_units),
            external_spools: scrub_material_json(snapshot.external_spools),
            active_tray: snapshot.active_tray.map(scrub_material_json),
            observed_at: snapshot.observed_at,
        }
    }
}

impl From<CommandRecord> for PrinterEventCommand {
    fn from(command: CommandRecord) -> Self {
        Self {
            id: command.id.to_string(),
            tenant_id: command.tenant_id.to_string(),
            agent_id: command.agent_id.to_string(),
            printer_id: command.printer_id,
            kind: command.kind,
            status: command.status.to_string(),
            payload_json: command.payload_json,
            error: command.error,
            result_json: command.result_json,
            created_at: command.created_at,
            updated_at: command.updated_at,
        }
    }
}

fn scrub_material_json(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(scrub_material_json).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter_map(|(key, value)| {
                    (!credential_key(&key)).then(|| (key, scrub_material_json(value)))
                })
                .collect(),
        ),
        value => value,
    }
}

fn credential_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    ["access_code", "password", "passwd", "token", "auth"]
        .iter()
        .any(|needle| key.contains(needle))
}

#[derive(Debug, Clone)]
pub struct PrinterEventHub {
    senders: Arc<Mutex<HashMap<String, broadcast::Sender<PrinterEvent>>>>,
    metrics: MetricsState,
}

impl PrinterEventHub {
    pub fn new() -> Self {
        Self::with_metrics(MetricsState::new())
    }

    pub fn with_metrics(metrics: MetricsState) -> Self {
        Self {
            senders: Arc::new(Mutex::new(HashMap::new())),
            metrics,
        }
    }

    pub async fn subscribe(&self, tenant_id: TenantId) -> broadcast::Receiver<PrinterEvent> {
        self.sender(tenant_id).await.subscribe()
    }

    pub async fn track_subscription(&self, tenant_id: TenantId) -> SubscriptionGuard {
        self.metrics.subscription_started(tenant_id).await
    }

    pub async fn publish_local(&self, tenant_id: TenantId, event: PrinterEvent) {
        let sender = self.sender(tenant_id).await;
        let _ = sender.send(event);
    }

    async fn sender(&self, tenant_id: TenantId) -> broadcast::Sender<PrinterEvent> {
        let mut senders = self.senders.lock().await;
        senders
            .entry(tenant_id.to_string())
            .or_insert_with(|| broadcast::channel(128).0)
            .clone()
    }
}

impl Default for PrinterEventHub {
    fn default() -> Self {
        Self::new()
    }
}
