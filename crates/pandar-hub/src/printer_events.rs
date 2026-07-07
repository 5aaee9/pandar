use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use pandar_core::{CommandRecord, Printer, PrinterNozzleTemperature, TenantId};
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};
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
    pub nozzle_temperatures: Vec<PrinterNozzleTemperature>,
    pub active_nozzle: Option<String>,
    pub bed_temperature_celsius: Option<String>,
    pub bed_target_temperature_celsius: Option<String>,
    pub chamber_temperature_celsius: Option<String>,
    pub chamber_light_on: Option<bool>,
    pub materials: Option<PrinterEventMaterials>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrinterEventMaterials {
    pub ams_units: PrinterEventMaterialJson,
    pub external_spools: PrinterEventMaterialJson,
    pub active_tray: Option<PrinterEventMaterialJson>,
    pub observed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PrinterEventMaterialJson {
    Object(BTreeMap<String, PrinterEventMaterialJson>),
    Array(Vec<PrinterEventMaterialJson>),
    String(String),
    Number(Number),
    Bool(bool),
    Null,
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
        nozzle_temperatures: printer.nozzle_temperatures,
        active_nozzle: printer.active_nozzle,
        bed_temperature_celsius: printer.bed_temperature_celsius,
        bed_target_temperature_celsius: printer.bed_target_temperature_celsius,
        chamber_temperature_celsius: printer.chamber_temperature_celsius,
        chamber_light_on: printer.chamber_light_on,
        materials: materials.map(PrinterEventMaterials::from),
    }
}

impl From<MaterialSnapshot> for PrinterEventMaterials {
    fn from(snapshot: MaterialSnapshot) -> Self {
        Self {
            ams_units: material_json(snapshot.ams_units).scrubbed(),
            external_spools: material_json(snapshot.external_spools).scrubbed(),
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

fn scrub_material_json(value: Value) -> PrinterEventMaterialJson {
    material_json(value).scrubbed()
}

fn material_json(value: Value) -> PrinterEventMaterialJson {
    serde_json::from_value::<PrinterEventMaterialJson>(value)
        .expect("printer material JSON is representable")
}

impl PrinterEventMaterialJson {
    fn scrubbed(self) -> Self {
        match self {
            Self::Array(values) => Self::Array(values.into_iter().map(Self::scrubbed).collect()),
            Self::Object(map) => Self::Object(
                map.into_iter()
                    .filter_map(|(key, value)| {
                        (!credential_key(&key)).then(|| (key, value.scrubbed()))
                    })
                    .collect(),
            ),
            value => value,
        }
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
