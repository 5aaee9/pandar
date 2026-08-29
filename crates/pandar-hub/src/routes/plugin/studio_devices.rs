use std::collections::HashMap;

use pandar_core::compatibility::{normalize_model, studio_local_camera_supported};
use pandar_core::{
    AgentId, BambuNozzleSystem, PrinterFirmwareState, PrinterNozzleTemperature, TenantId,
};
use serde::Serialize;

use crate::{
    AppState,
    printer_events::PrinterEventMaterials,
    repositories::{PrinterHms, PrinterWithLiveStatus},
    routes::ApiError,
    sessions::CurrentAgentSessionSnapshot,
};
use pandar_protocol::agent::v1::AgentCapability;

#[derive(Debug, Serialize)]
pub(crate) struct PluginPrinterListResponse {
    pub(super) message: &'static str,
    pub(super) devices: Vec<PluginPrinterResponse>,
}

#[derive(Debug, Serialize)]
pub(in crate::routes) struct PluginPrinterResponse {
    dev_id: String,
    fun: String,
    dev_name: String,
    name: String,
    dev_model_name: Option<String>,
    model: Option<String>,
    dev_online: bool,
    online: bool,
    task_status: String,
    state: String,
    gcode_state: Option<String>,
    mc_percent: Option<u8>,
    mc_remaining_time: Option<u32>,
    layer_num: Option<u32>,
    total_layer_num: Option<u32>,
    task_id: Option<String>,
    subtask_id: Option<String>,
    gcode_file: Option<String>,
    subtask_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    print_error: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    job_id: Option<String>,
    hms: Vec<PrinterHms>,
    pandar_printer_id: String,
    nozzle_temperatures: Vec<PrinterNozzleTemperature>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nozzle_system: Option<BambuNozzleSystem>,
    active_nozzle: Option<String>,
    bed_temperature_celsius: Option<String>,
    bed_target_temperature_celsius: Option<String>,
    chamber_temperature_celsius: Option<String>,
    chamber_target_temperature_celsius: Option<String>,
    chamber_light_on: Option<bool>,
    studio_local_camera: bool,
    materials: Option<PrinterEventMaterials>,
    #[serde(skip_serializing_if = "Option::is_none")]
    firmware: Option<PrinterFirmwareState>,
}

impl PluginPrinterResponse {
    pub(in crate::routes) fn pandar_printer_id(&self) -> &str {
        &self.pandar_printer_id
    }
}

pub(in crate::routes) async fn plugin_printer_devices(
    state: &AppState,
    tenant_id: TenantId,
) -> Result<Vec<PluginPrinterResponse>, ApiError> {
    let materials_by_printer_id = materials_by_printer(state, tenant_id).await?;
    let printers = state
        .printers()
        .list_with_live_status_for_tenant(tenant_id)
        .await?;
    let sessions = current_agent_projection_snapshots(state, tenant_id).await?;
    Ok(printers
        .into_iter()
        .map(|entry| {
            let materials = materials_by_printer_id.get(&entry.printer.id).cloned();
            let session = sessions.get(&entry.printer.agent_id);
            studio_printer_record(entry, materials, session)
        })
        .collect())
}

async fn materials_by_printer(
    state: &AppState,
    tenant_id: TenantId,
) -> Result<HashMap<String, PrinterEventMaterials>, ApiError> {
    Ok(state
        .materials()
        .list_for_tenant(tenant_id)
        .await?
        .into_iter()
        .map(|snapshot| {
            (
                snapshot.printer_id.clone(),
                PrinterEventMaterials::from(snapshot),
            )
        })
        .collect())
}

async fn current_agent_projection_snapshots(
    state: &AppState,
    tenant_id: TenantId,
) -> Result<HashMap<AgentId, CurrentAgentSessionSnapshot>, ApiError> {
    let persisted = state
        .agents()
        .current_session_ids_for_tenant(tenant_id)
        .await?;
    let mut sessions = state.sessions().current_session_snapshots(tenant_id).await;
    sessions.retain(|agent_id, session| persisted.get(agent_id) == Some(&session.persisted_id()));
    Ok(sessions)
}

/// Resolves one projection change into the authoritative Studio-facing record.
/// A printer that no longer exists resolves to a removal carrying its identity.
pub(in crate::routes) async fn studio_projection_record(
    state: &AppState,
    tenant_id: TenantId,
    change: &crate::printer_events::PrinterProjectionChange,
) -> Result<StudioProjectionRecord, ApiError> {
    let printers = state
        .printers()
        .list_with_live_status_for_tenant(tenant_id)
        .await?;
    let Some(entry) = printers
        .into_iter()
        .find(|entry| entry.printer.id == change.printer_id)
    else {
        return Ok(StudioProjectionRecord::Removed {
            dev_id: change.serial_number.clone(),
            pandar_printer_id: change.printer_id.clone(),
        });
    };
    let materials = state
        .materials()
        .list_for_tenant(tenant_id)
        .await?
        .into_iter()
        .find(|snapshot| snapshot.printer_id == change.printer_id)
        .map(PrinterEventMaterials::from);
    let sessions = current_agent_projection_snapshots(state, tenant_id).await?;
    let session = sessions.get(&entry.printer.agent_id);
    Ok(StudioProjectionRecord::Upsert(Box::new(
        studio_printer_record(entry, materials, session),
    )))
}

#[derive(Debug)]
pub(in crate::routes) enum StudioProjectionRecord {
    Upsert(Box<PluginPrinterResponse>),
    Removed {
        dev_id: String,
        pandar_printer_id: String,
    },
}

/// Builds one complete Studio-facing printer record from one current Agent
/// session snapshot, keeping online and capability projections coherent.
fn studio_printer_record(
    entry: PrinterWithLiveStatus,
    materials: Option<PrinterEventMaterials>,
    session: Option<&CurrentAgentSessionSnapshot>,
) -> PluginPrinterResponse {
    let firmware = entry.firmware;
    let printer = entry.printer;
    let live_status = entry.live_status;
    let current_session_id = session.map(CurrentAgentSessionSnapshot::persisted_id);
    let online = current_session_id.as_deref() == printer.mqtt_presence_session_id.as_deref()
        && studio_online_from_status(&printer.status);
    let studio_model_name = printer.model.as_deref().map(studio_model_id);
    let studio_local_camera = online
        && studio_local_camera_supported(printer.model.as_deref())
        && session.is_some_and(|session| session.supports(AgentCapability::StudioLocalCamera));
    let fun = session
        .filter(|session| session.supports(AgentCapability::RequiredDeviceFeatures))
        .filter(|session| {
            printer.bambu_device_features_session_id.as_deref()
                == Some(session.persisted_id().as_str())
        })
        .and(printer.bambu_device_features)
        .map_or_else(|| "0".to_owned(), |features| features.to_hex());
    let nozzle_system = (printer
        .model
        .as_deref()
        .and_then(normalize_model)
        .as_deref()
        == Some("H2C"))
    .then(|| {
        session
            .filter(|session| session.supports(AgentCapability::H2cAutoNozzleMapping))
            .filter(|session| {
                printer.bambu_nozzle_system_session_id.as_deref()
                    == Some(session.persisted_id().as_str())
            })
            .and(printer.bambu_nozzle_system.clone())
    })
    .flatten();
    let firmware = session
        .filter(|session| session.supports(AgentCapability::FirmwareControl))
        .filter(|session| firmware.session_id.as_deref() == Some(session.persisted_id().as_str()))
        .filter(|_| firmware.generation.is_some())
        .map(|_| firmware);
    PluginPrinterResponse {
        dev_id: printer.serial_number.clone(),
        fun,
        dev_name: printer.name.clone(),
        name: printer.name,
        dev_model_name: studio_model_name,
        model: printer.model,
        dev_online: online,
        online,
        task_status: printer.status.clone(),
        state: printer.status,
        gcode_state: live_status.gcode_state,
        mc_percent: live_status.progress_percent,
        mc_remaining_time: live_status.remaining_time_minutes,
        layer_num: live_status.current_layer,
        total_layer_num: live_status.total_layers,
        task_id: live_status.task_id,
        subtask_id: live_status.subtask_id,
        gcode_file: live_status.gcode_file,
        subtask_name: live_status.subtask_name,
        print_error: live_status.print_error,
        job_id: live_status.printer_job_id,
        hms: live_status.hms,
        nozzle_temperatures: printer.nozzle_temperatures,
        nozzle_system,
        active_nozzle: printer.active_nozzle,
        bed_temperature_celsius: printer.bed_temperature_celsius,
        bed_target_temperature_celsius: printer.bed_target_temperature_celsius,
        chamber_temperature_celsius: printer.chamber_temperature_celsius,
        chamber_target_temperature_celsius: printer.chamber_target_temperature_celsius,
        chamber_light_on: printer.chamber_light_on,
        studio_local_camera,
        materials,
        firmware,
        pandar_printer_id: printer.id,
    }
}

fn studio_online_from_status(status: &str) -> bool {
    let normalized = status.trim().to_ascii_lowercase();
    !matches!(normalized.as_str(), "offline" | "unknown")
}

fn studio_model_id(model: &str) -> String {
    let studio_id = match normalize_model(model).as_deref() {
        Some("A1_MINI") => "N1",
        Some("A1") => "N2S",
        Some("X1C") => "BL-P001",
        Some("X1") => "BL-P002",
        Some("P1P") => "C11",
        Some("P1S") => "C12",
        Some("X1E") => "C13",
        Some("X2D") => "N6",
        Some("P2S") => "N7",
        Some("A2L") => "N9",
        Some("H2C") => "O1C2",
        Some("H2D") => "O1D",
        Some("H2D_PRO") => "O1E",
        Some("H2S") => "O1S",
        _ => model,
    };
    studio_id.to_owned()
}
