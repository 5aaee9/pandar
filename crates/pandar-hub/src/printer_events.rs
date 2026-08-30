use std::{
    collections::HashMap,
    sync::{Arc, Mutex as StdMutex, MutexGuard},
};

use pandar_core::{
    BambuNozzleSystem, CommandRecord, DiagnosticCompatibility, PrinterCoolingSystem,
    PrinterNozzleTemperature, TenantId,
    compatibility::{compatibility_for_model, normalize_model},
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, broadcast, watch};

use crate::{
    metrics::{MetricsState, SubscriptionGuard},
    repositories::{MaterialSnapshot, PrinterHms, PrinterWithLiveStatus},
    sessions::SessionRegistry,
};
use pandar_protocol::agent::v1::AgentCapability;

mod materials;
mod projection;

pub use crate::job_projection::JobProjection as PrinterEventJob;
pub use projection::PrinterProjectionChange;
pub(crate) use projection::ProjectionSubscription;

pub use materials::{PrinterEventMaterialJson, PrinterEventMaterials};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PrinterEvent {
    #[serde(rename = "printer_snapshot")]
    PrinterSnapshot { printer: Box<PrinterEventPrinter> },
    #[serde(rename = "job_progress")]
    JobProgress { job: Box<PrinterEventJob> },
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
    #[serde(default = "unknown_printer_compatibility")]
    pub compatibility: DiagnosticCompatibility,
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

fn unknown_printer_compatibility() -> DiagnosticCompatibility {
    compatibility_for_model(None)
}

pub fn printer_event_printer(
    printer: PrinterWithLiveStatus,
    materials: Option<MaterialSnapshot>,
) -> PrinterEventPrinter {
    let state_revision = printer.state_revision;
    let live_status = printer.live_status;
    let printer = printer.printer;
    let compatibility = compatibility_for_model(printer.model.as_deref());
    PrinterEventPrinter {
        id: printer.id,
        tenant_id: printer.tenant_id.to_string(),
        agent_id: printer.agent_id.to_string(),
        serial_number: printer.serial_number,
        name: printer.name,
        model: printer.model,
        compatibility,
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

#[derive(Debug, Clone)]
pub struct PrinterEventHub {
    senders: Arc<Mutex<HashMap<String, broadcast::Sender<PrinterEvent>>>>,
    projection: projection::ProjectionEventHub,
    metrics: MetricsState,
    /// Per-tenant epochs, bumped when one tenant's event flow may have lost
    /// events; only that tenant's sockets resynchronize.
    epochs: Arc<StdMutex<HashMap<String, watch::Sender<u64>>>>,
    /// Process-wide epoch for control-plane-wide failures (subscribe loss,
    /// receive errors) that may affect every tenant's event flow.
    global_epoch: watch::Sender<u64>,
    epoch_gate: PrinterEventEpochGate,
    capacity: usize,
}

/// Watches the per-tenant and process-wide epochs so a WebSocket closes when
/// either its tenant's event flow or the whole control plane needs a resync.
#[derive(Debug, Clone)]
pub(crate) struct PrinterEventEpoch {
    tenant: watch::Receiver<u64>,
    global: watch::Receiver<u64>,
}

impl PrinterEventEpoch {
    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
        tokio::select! {
            biased;
            changed = self.tenant.changed() => changed,
            changed = self.global.changed() => changed,
        }
    }

    pub fn has_changed(&mut self) -> Result<bool, watch::error::RecvError> {
        if self.tenant.has_changed()? {
            return Ok(true);
        }
        self.global.has_changed()
    }
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
            epochs: Arc::new(StdMutex::new(HashMap::new())),
            global_epoch: watch::channel(0).0,
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

    pub(crate) fn subscribe_epoch(&self, tenant_id: TenantId) -> PrinterEventEpoch {
        let tenant = self.epoch(tenant_id);
        PrinterEventEpoch {
            tenant,
            global: self.global_epoch.subscribe(),
        }
    }

    pub(crate) fn epoch_gate(&self) -> PrinterEventEpochGate {
        self.epoch_gate.clone()
    }

    fn epoch(&self, tenant_id: TenantId) -> watch::Receiver<u64> {
        let mut epochs = self
            .epochs
            .lock()
            .expect("printer event epoch map should not be poisoned");
        epochs
            .entry(tenant_id.to_string())
            .or_insert_with(|| watch::channel(0).0)
            .subscribe()
    }

    /// Invalidate one tenant's epoch after a failed publish so only that
    /// tenant's sockets close and resynchronize.
    pub fn invalidate_epoch(&self, tenant_id: TenantId) {
        let _gate = self.epoch_gate.lock();
        let epoch = {
            let mut epochs = self
                .epochs
                .lock()
                .expect("printer event epoch map should not be poisoned");
            epochs
                .entry(tenant_id.to_string())
                .or_insert_with(|| watch::channel(0).0)
                .clone()
        };
        epoch.send_modify(|value| *value = value.wrapping_add(1));
    }

    /// Invalidate every tenant's epoch after a control-plane-wide failure.
    pub fn invalidate_all_epochs(&self) {
        let _gate = self.epoch_gate.lock();
        self.global_epoch
            .send_modify(|value| *value = value.wrapping_add(1));
    }

    /// Deliver one control-plane-replicated event to local subscribers. Only
    /// the control-plane consumer may call this; producers must go through
    /// [`AppState::publish_printer_event`] so every replica receives the event.
    pub(crate) async fn deliver_local(&self, tenant_id: TenantId, event: PrinterEvent) {
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
