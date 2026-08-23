use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex as StdMutex, MutexGuard},
};

use pandar_core::{
    BambuNozzleSystem, CommandRecord, PrinterCoolingSystem, PrinterNozzleTemperature, TenantId,
    compatibility::normalize_model,
};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use tokio::sync::{Mutex, broadcast, watch};

use crate::{
    metrics::{MetricsState, SubscriptionGuard},
    protocol::agent::v1::AgentCapability,
    repositories::{MaterialJsonValue, MaterialSnapshot, PrinterHms, PrinterWithLiveStatus},
    routes::jobs::JobResponse,
    sessions::SessionRegistry,
};

mod projection;
pub use projection::PrinterProjectionChange;
pub(crate) use projection::ProjectionSubscription;

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
    pub chamber_target_temperature_celsius: Option<String>,
    pub chamber_light_on: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooling_system: Option<PrinterCoolingSystem>,
    pub materials: Option<PrinterEventMaterials>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nozzle_system: Option<BambuNozzleSystem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub print: Option<PrinterEventPrint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrinterEventPrint {
    pub task_generation: u64,
    pub error_generation: u64,
    pub job_state: Option<u32>,
    pub gcode_state: Option<String>,
    pub task_id: Option<String>,
    pub subtask_id: Option<String>,
    pub progress_percent: Option<u8>,
    pub speed_level: Option<u8>,
    pub remaining_time_minutes: Option<u32>,
    pub current_layer: Option<u32>,
    pub total_layers: Option<u32>,
    pub gcode_file: Option<String>,
    pub subtask_name: Option<String>,
    pub print_error: Option<u32>,
    pub printer_job_id: Option<String>,
    pub hms: Vec<PrinterHms>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrinterEventMaterials {
    pub ams_units: PrinterEventMaterialJson,
    pub external_spools: PrinterEventMaterialJson,
    pub active_tray: Option<PrinterEventMaterialJson>,
    pub filament_switch_installed: Option<bool>,
    pub cfg: Option<String>,
    pub aux: Option<String>,
    pub stat: Option<String>,
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

/// Drops the stored H2C nozzle system unless it was reported by the current
/// H2C-capable Agent session, so a replaced session cannot advertise a prior
/// session's rack state to dashboard clients.
pub async fn fence_printer_nozzle_system(
    sessions: &SessionRegistry,
    tenant_id: TenantId,
    mut printer: PrinterWithLiveStatus,
) -> PrinterWithLiveStatus {
    let current = printer
        .printer
        .model
        .as_deref()
        .and_then(normalize_model)
        .as_deref()
        == Some("H2C")
        && matches!(
            sessions
                .current_token_for_capability(
                    tenant_id,
                    printer.printer.agent_id,
                    AgentCapability::H2cAutoNozzleMapping,
                )
                .await,
            Some(token)
                if printer.printer.bambu_nozzle_system_session_id.as_deref()
                    == Some(token.persisted_id().as_str())
        );
    if !current {
        printer.printer.bambu_nozzle_system = None;
    }
    printer
}

pub fn printer_event_printer(
    printer: PrinterWithLiveStatus,
    materials: Option<MaterialSnapshot>,
) -> PrinterEventPrinter {
    let state_revision = printer.state_revision;
    let live_status = printer.live_status;
    let printer = printer.printer;
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
        chamber_target_temperature_celsius: printer.chamber_target_temperature_celsius,
        chamber_light_on: printer.chamber_light_on,
        cooling_system: printer.cooling_system,
        materials: materials.map(PrinterEventMaterials::from),
        nozzle_system: printer.bambu_nozzle_system,
        state_revision: Some(state_revision),
        print: Some(PrinterEventPrint {
            task_generation: live_status.task_generation,
            error_generation: live_status.error_generation,
            job_state: live_status.job_attr.map(|job_attr| (job_attr >> 4) & 0x0f),
            gcode_state: live_status.gcode_state,
            task_id: live_status.task_id,
            subtask_id: live_status.subtask_id,
            progress_percent: live_status.progress_percent,
            speed_level: live_status.speed_level,
            remaining_time_minutes: live_status.remaining_time_minutes,
            current_layer: live_status.current_layer,
            total_layers: live_status.total_layers,
            gcode_file: live_status.gcode_file,
            subtask_name: live_status.subtask_name,
            print_error: live_status.print_error,
            printer_job_id: live_status.printer_job_id,
            hms: live_status.hms,
        }),
    }
}

impl From<MaterialSnapshot> for PrinterEventMaterials {
    fn from(snapshot: MaterialSnapshot) -> Self {
        Self {
            ams_units: PrinterEventMaterialJson::from(snapshot.ams_units).scrubbed(),
            external_spools: PrinterEventMaterialJson::from(snapshot.external_spools).scrubbed(),
            active_tray: snapshot.active_tray.map(scrub_material_json),
            filament_switch_installed: snapshot.filament_switch_installed,
            observed_at: snapshot.observed_at,
            cfg: snapshot.cfg,
            aux: snapshot.aux,
            stat: snapshot.stat,
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

fn scrub_material_json(value: MaterialJsonValue) -> PrinterEventMaterialJson {
    PrinterEventMaterialJson::from(value).scrubbed()
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

impl From<MaterialJsonValue> for PrinterEventMaterialJson {
    fn from(value: MaterialJsonValue) -> Self {
        match value {
            MaterialJsonValue::Object(object) => Self::Object(
                object
                    .into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect(),
            ),
            MaterialJsonValue::Array(values) => {
                Self::Array(values.into_iter().map(Self::from).collect())
            }
            MaterialJsonValue::String(value) => Self::String(value),
            MaterialJsonValue::Number(value) => Self::Number(value),
            MaterialJsonValue::Bool(value) => Self::Bool(value),
            MaterialJsonValue::Null => Self::Null,
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
    projection: projection::ProjectionEventHub,
    metrics: MetricsState,
    epoch: watch::Sender<u64>,
    epoch_gate: PrinterEventEpochGate,
    capacity: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PrinterEventEpochGate(Arc<StdMutex<()>>);

impl PrinterEventEpochGate {
    pub(crate) fn lock(&self) -> MutexGuard<'_, ()> {
        self.0
            .lock()
            .expect("printer event epoch gate mutex should not be poisoned")
    }
}

impl PrinterEventHub {
    pub fn new() -> Self {
        Self::with_metrics(MetricsState::new())
    }

    pub fn with_metrics(metrics: MetricsState) -> Self {
        Self::with_metrics_and_capacity(metrics, 128)
    }

    fn with_metrics_and_capacity(metrics: MetricsState, capacity: usize) -> Self {
        Self {
            senders: Arc::new(Mutex::new(HashMap::new())),
            projection: projection::ProjectionEventHub::new(capacity),
            metrics,
            epoch: watch::channel(0).0,
            epoch_gate: PrinterEventEpochGate::default(),
            capacity,
        }
    }

    pub async fn subscribe_projection_changes(
        &self,
        tenant_id: TenantId,
    ) -> ProjectionSubscription {
        self.projection.subscribe(tenant_id).await
    }

    pub async fn publish_local_projection_change(
        &self,
        tenant_id: TenantId,
        change: PrinterProjectionChange,
    ) {
        self.projection.publish(tenant_id, change).await;
    }

    #[cfg(test)]
    pub(crate) fn with_capacity_for_tests(capacity: usize) -> Self {
        Self::with_metrics_and_capacity(MetricsState::new(), capacity)
    }

    pub async fn subscribe(&self, tenant_id: TenantId) -> broadcast::Receiver<PrinterEvent> {
        self.sender(tenant_id).await.subscribe()
    }

    pub async fn track_subscription(&self, tenant_id: TenantId) -> SubscriptionGuard {
        self.metrics.subscription_started(tenant_id).await
    }

    pub fn subscribe_epoch(&self) -> watch::Receiver<u64> {
        self.epoch.subscribe()
    }

    pub(crate) fn epoch_gate(&self) -> PrinterEventEpochGate {
        self.epoch_gate.clone()
    }

    pub fn invalidate_epoch(&self) {
        let _gate = self.epoch_gate.lock();
        self.epoch
            .send_modify(|value| *value = value.wrapping_add(1));
    }

    pub async fn publish_local(&self, tenant_id: TenantId, event: PrinterEvent) {
        let sender = self.sender(tenant_id).await;
        let _ = sender.send(event);
    }

    #[cfg(test)]
    pub(crate) async fn publish_local_burst_for_tests(
        &self,
        tenant_id: TenantId,
        events: Vec<PrinterEvent>,
    ) {
        let sender = self.sender(tenant_id).await;
        for event in events {
            let _ = sender.send(event);
        }
    }

    async fn sender(&self, tenant_id: TenantId) -> broadcast::Sender<PrinterEvent> {
        let mut senders = self.senders.lock().await;
        senders
            .entry(tenant_id.to_string())
            .or_insert_with(|| broadcast::channel(self.capacity).0)
            .clone()
    }
}

impl Default for PrinterEventHub {
    fn default() -> Self {
        Self::new()
    }
}
